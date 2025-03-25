use std::{path::Path, time::Instant};

use anyhow::anyhow;
use chrono::Utc;
use cln_plugin::Plugin;
use cln_rpc::{
    model::requests::{
        DecodeRequest, GetinfoRequest, ListsendpaysIndex, ListsendpaysRequest, ListsendpaysStatus,
    },
    ClnRpc,
};
use serde_json::Map;

use crate::{
    parse::get_maxfee,
    structs::{Paycmd, PluginState},
};

pub async fn budget_check(
    plugin: Plugin<PluginState>,
    params: &Map<String, serde_json::Value>,
    paycmd: Paycmd,
) -> Result<(), anyhow::Error> {
    let config = plugin.state().config.lock().clone();
    if config.budget_amount_msat.is_none() || config.budget_per.is_none() {
        return Ok(());
    }
    let now = Instant::now();
    let budget_amount_msat = config.budget_amount_msat.unwrap().msat();
    let budget_per = config.budget_per.unwrap();
    let now_stamp = Utc::now().timestamp() as u64;
    let time_window = now_stamp - budget_per;
    let pending_deadline = now_stamp - 2592000;
    let mut budget_amount_msat_used = 0;
    let mut pay_created_index = None;

    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let invoice = match paycmd {
        Paycmd::Pay => params.get("bolt11").unwrap().as_str().unwrap().to_owned(),
        Paycmd::Xpay | Paycmd::Renepay => params
            .get("invstring")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned(),
    };
    let invoice_decoded = rpc.call_typed(&DecodeRequest { string: invoice }).await?;
    let invoice_amt_msat = match invoice_decoded.item_type {
        cln_rpc::model::responses::DecodeType::BOLT12_INVOICE => {
            invoice_decoded.invoice_amount_msat.unwrap().msat()
        }
        cln_rpc::model::responses::DecodeType::BOLT11_INVOICE => {
            invoice_decoded.amount_msat.unwrap().msat()
        }
        _ => return Err(anyhow!("Wrong invoice type decoded!")),
    };

    let getinfo = rpc.call_typed(&GetinfoRequest {}).await?;

    let maxfee = get_maxfee(
        params.get("maxfee").cloned(),
        params.get("maxfeepercent").cloned(),
        params.get("exemptfee").cloned(),
        invoice_amt_msat,
    )?;

    budget_amount_msat_used += invoice_amt_msat;
    budget_amount_msat_used += maxfee;

    if budget_amount_msat_used > budget_amount_msat {
        return Err(anyhow!(
            "Invoice amount+fee is greater than budget already!"
        ));
    }

    #[allow(clippy::clone_on_copy)]
    let old_index = plugin.state().pay_index.lock().clone();

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

    for pp in pending_pays.iter() {
        if let Some(dest) = pp.destination {
            if dest == getinfo.id {
                continue;
            }
        }
        if pp.created_at < pending_deadline {
            continue;
        }
        budget_amount_msat_used += pp.amount_sent_msat.msat();

        if let Some(ppci) = pp.created_index {
            if let Some(ci) = pay_created_index {
                if ppci < ci {
                    pay_created_index = Some(ppci)
                }
            } else {
                pay_created_index = Some(ppci)
            }
        }
    }

    for cp in completed_pays.iter() {
        if let Some(dest) = cp.destination {
            if dest == getinfo.id {
                continue;
            }
        }
        if cp.completed_at.unwrap() < time_window {
            continue;
        }
        budget_amount_msat_used += cp.amount_sent_msat.msat();

        if let Some(cpci) = cp.created_index {
            if let Some(ci) = pay_created_index {
                if cpci < ci {
                    pay_created_index = Some(cpci)
                }
            } else {
                pay_created_index = Some(cpci)
            }
        }
    }

    if let Some(index) = pay_created_index {
        *plugin.state().pay_index.lock() = index;
    }

    if budget_amount_msat_used > budget_amount_msat {
        return Err(anyhow!(
            "Budget would be exceeded! {}msat / {}msat",
            budget_amount_msat_used,
            budget_amount_msat
        ));
    }
    log::info!(
        "Within budget! {}msat / {}msat (check took {}ms)",
        budget_amount_msat_used,
        budget_amount_msat,
        now.elapsed().as_millis()
    );
    Ok(())
}
