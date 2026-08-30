use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use chrono::Utc;
use cln_plugin::Plugin;
use cln_rpc::{
    ClnRpc,
    model::requests::{
        DecodeRequest,
        GetinfoRequest,
        ListsendpaysIndex,
        ListsendpaysRequest,
        ListsendpaysStatus,
    },
};
use serde_json::Map;

use crate::{
    parse::get_maxfee,
    structs::{Paycmd, PluginState},
};

const RESERVATION_EXPIRY: Duration = Duration::from_secs(10);

pub async fn budget_check(
    plugin: Plugin<PluginState>,
    params: &Map<String, serde_json::Value>,
    paycmd: Paycmd,
) -> Result<(), anyhow::Error> {
    let config = plugin.state().config.lock().clone();
    if config.budget_amount_msat.is_none() || config.budget_per.is_none() {
        return Ok(());
    }
    let _budget_lock = plugin.state().budget_lock.lock().await;
    let now = Instant::now();
    let budget_amount_msat = config.budget_amount_msat.unwrap().msat();
    let budget_per = config.budget_per.unwrap();
    let now_stamp = Utc::now().timestamp() as u64;
    let time_window = now_stamp - budget_per;
    let pending_deadline = now_stamp - 2_592_000;
    let mut budget_amount_msat_used = 0;
    let mut pay_created_index = None;

    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let invoice = match paycmd {
        Paycmd::Pay => params
            .get(config.payargs.first().unwrap())
            .ok_or_else(|| {
                anyhow!(
                    "first parameter `{}` for `pay` not found",
                    config.payargs.first().unwrap()
                )
            })?
            .as_str()
            .unwrap()
            .to_owned(),
        Paycmd::Xpay => params
            .get(config.xpayargs.first().unwrap())
            .ok_or_else(|| {
                anyhow!(
                    "first parameter `{}` for `xpay` not found",
                    config.xpayargs.first().unwrap()
                )
            })?
            .as_str()
            .unwrap()
            .to_owned(),
        Paycmd::Renepay => params
            .get(config.renepayargs.first().unwrap())
            .ok_or_else(|| {
                anyhow!(
                    "first paramteer `{}` for `renepay` not found",
                    config.renepayargs.first().unwrap()
                )
            })?
            .as_str()
            .unwrap()
            .to_owned(),
    };
    let invoice_decoded = rpc.call_typed(&DecodeRequest { string: invoice }).await?;
    let (invoice_amt_msat, payment_hash) = match invoice_decoded.item_type {
        cln_rpc::model::responses::DecodeType::BOLT12_INVOICE => (
            invoice_decoded.invoice_amount_msat.map(|a| a.msat()),
            invoice_decoded
                .invoice_payment_hash
                .ok_or_else(|| anyhow!("No payment_hash in decoded invoice!"))?,
        ),
        cln_rpc::model::responses::DecodeType::BOLT11_INVOICE => (
            invoice_decoded.amount_msat.map(|a| a.msat()),
            invoice_decoded
                .payment_hash
                .ok_or_else(|| anyhow!("No payment_hash in decoded invoice!"))?
                .to_string(),
        ),
        _ => return Err(anyhow!("Wrong invoice type decoded!")),
    };

    let invoice_amt_msat = if let Some(inv_amt) = invoice_amt_msat {
        inv_amt
    } else {
        params
            .get("amount_msat")
            .ok_or_else(|| anyhow!("amountless invoice with no amount_msat given"))?
            .as_u64()
            .ok_or_else(|| anyhow!("amount_msat is not an integer"))?
    };

    let mut reserved: HashMap<String, (u64, Instant)> = {
        let mut budget_reserved = plugin.state().budget_reserved.lock();
        budget_reserved.retain(|_, (_, approved_at)| approved_at.elapsed() < RESERVATION_EXPIRY);
        budget_reserved.clone()
    };
    reserved.remove(&payment_hash);

    let getinfo = rpc.call_typed(&GetinfoRequest {}).await?;

    let maxfee = get_maxfee(
        params.get("maxfee").cloned(),
        params.get("maxfeepercent").cloned(),
        params.get("exemptfee").cloned(),
        invoice_amt_msat,
    )?;

    budget_amount_msat_used += invoice_amt_msat;
    budget_amount_msat_used = budget_amount_msat_used.saturating_add(maxfee);

    if budget_amount_msat_used > budget_amount_msat {
        return Err(anyhow!(
            "Invoice amount+fee is greater than budget already!"
        ));
    }

    let old_index = *plugin.state().pay_index.lock();

    let pending_pays = rpc
        .call_typed(&ListsendpaysRequest {
            bolt11: None,
            index: Some(ListsendpaysIndex::CREATED),
            limit: None,
            payment_hash: None,
            start: Some(old_index),
            status: Some(ListsendpaysStatus::PENDING),
        })
        .await?
        .payments;
    let completed_pays = rpc
        .call_typed(&ListsendpaysRequest {
            bolt11: None,
            index: Some(ListsendpaysIndex::CREATED),
            limit: None,
            payment_hash: None,
            start: Some(old_index),
            status: Some(ListsendpaysStatus::COMPLETE),
        })
        .await?
        .payments;

    for pp in &pending_pays {
        reserved.remove(&pp.payment_hash.to_string());
        if let Some(dest) = pp.destination {
            if dest == getinfo.id {
                continue;
            }
        }
        if pp.created_at < pending_deadline {
            continue;
        }
        budget_amount_msat_used =
            budget_amount_msat_used.saturating_add(pp.amount_sent_msat.msat());

        if let Some(ci) = pay_created_index {
            if pp.created_index < ci {
                pay_created_index = Some(pp.created_index);
            }
        } else {
            pay_created_index = Some(pp.created_index);
        }
    }

    for cp in &completed_pays {
        reserved.remove(&cp.payment_hash.to_string());
        if let Some(dest) = cp.destination {
            if dest == getinfo.id {
                continue;
            }
        }
        if cp.completed_at.unwrap() < time_window {
            continue;
        }
        budget_amount_msat_used =
            budget_amount_msat_used.saturating_add(cp.amount_sent_msat.msat());

        if let Some(ci) = pay_created_index {
            if cp.created_index < ci {
                pay_created_index = Some(cp.created_index);
            }
        } else {
            pay_created_index = Some(cp.created_index);
        }
    }

    for (amount, _) in reserved.values() {
        budget_amount_msat_used = budget_amount_msat_used.saturating_add(*amount);
    }

    if let Some(index) = pay_created_index {
        *plugin.state().pay_index.lock() = index;
    }

    if budget_amount_msat_used > budget_amount_msat {
        return Err(anyhow!(
            "Budget would be exceeded! {budget_amount_msat_used}msat / {budget_amount_msat}msat"
        ));
    }

    plugin.state().budget_reserved.lock().insert(
        payment_hash,
        (invoice_amt_msat.saturating_add(maxfee), Instant::now()),
    );

    log::info!(
        "Within budget! {}msat / {}msat (check took {}ms)",
        budget_amount_msat_used,
        budget_amount_msat,
        now.elapsed().as_millis()
    );
    Ok(())
}
