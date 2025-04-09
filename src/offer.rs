use std::{collections::HashSet, net::SocketAddr, path::Path};

use anyhow::{anyhow, Error};
use cln_plugin::Plugin;
use cln_rpc::{
    model::requests::{DecodeRequest, FetchinvoiceRequest},
    primitives::Amount,
    ClnRpc,
};
use hickory_resolver::{
    config::{ResolverConfig, ResolverOpts},
    name_server::TokioConnectionProvider,
    proto::rr::RecordType,
    system_conf::read_system_conf,
    TokioResolver,
};
use serde_json::Map;

use crate::PluginState;

pub async fn fetch_invoice_bolt12(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    invstring: String,
    amount_msat: Option<Amount>,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let offer_decoded = rpc
        .call_typed(&DecodeRequest {
            string: invstring.clone(),
        })
        .await?;

    if offer_decoded.offer_currency.is_some() {
        return Err(anyhow!(
            "offers with non-BTC currencies are not supported by payany, \
        please fetch the invoice yourself"
        ));
    }

    if offer_decoded.offer_amount_msat.is_none() && amount_msat.is_none() {
        return Err(anyhow!(
            "offer has `any` amount, must specify `amount_msat`!"
        ));
    }

    if let Some(offer_amt) = offer_decoded.offer_amount_msat {
        if let Some(amt) = amount_msat {
            if offer_amt.msat() != amt.msat() {
                return Err(anyhow!(
                    "offer: not matching stated amount_msat: {} != {}",
                    offer_amt.msat(),
                    amt.msat()
                ));
            }
        }
    }

    let fetch_amount_msat = if offer_decoded.offer_amount_msat.is_some() {
        None
    } else {
        Some(amount_msat.unwrap())
    };
    let invoice = rpc
        .call_typed(&FetchinvoiceRequest {
            amount_msat: fetch_amount_msat,
            payer_metadata: None,
            payer_note: message,
            quantity: None,
            recurrence_counter: None,
            recurrence_label: None,
            recurrence_start: None,
            timeout: None,
            offer: invstring,
            bip353: None,
        })
        .await?;

    let invoice_decoded = rpc
        .call_typed(&DecodeRequest {
            string: invoice.invoice.clone(),
        })
        .await?;

    if let Some(offer_amt) = offer_decoded.offer_amount_msat {
        if invoice_decoded.invoice_amount_msat.unwrap().msat() != offer_amt.msat() {
            return Err(anyhow!(
                "offers: got invoice with different amount_msat than offer!: {} != {}",
                invoice_decoded.invoice_amount_msat.unwrap().msat(),
                offer_amt.msat()
            ));
        }
    }
    if let Some(amt) = amount_msat {
        if invoice_decoded.invoice_amount_msat.unwrap().msat() != amt.msat() {
            return Err(anyhow!(
                "offers: got invoice with different amount_msat than specified!: {} != {}",
                invoice_decoded.invoice_amount_msat.unwrap().msat(),
                amt.msat()
            ));
        }
    }

    params.remove("amount_msat");
    *params.get_mut(invstring_name).unwrap() = serde_json::Value::String(invoice.invoice);
    Ok(())
}

pub async fn fetch_invoice_bip353(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    user: &str,
    domain: &str,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let config = plugin.state().config.lock().clone();
    let (resolver_config, mut resolver_opts) = match config.dns_server {
        crate::structs::DnsServer::Google => {
            (ResolverConfig::google_https(), ResolverOpts::default())
        }
        crate::structs::DnsServer::Cloudflare => {
            (ResolverConfig::cloudflare_https(), ResolverOpts::default())
        }
        crate::structs::DnsServer::Quad9 => {
            (ResolverConfig::quad9_https(), ResolverOpts::default())
        }
        crate::structs::DnsServer::System => read_system_conf()?,
    };
    resolver_opts.validate = true;
    let mut resolver =
        TokioResolver::builder_with_config(resolver_config, TokioConnectionProvider::default());
    *resolver.options_mut() = resolver_opts;
    let resolver = resolver.build();

    log::debug!(
        "Using {:?} as DNS server(s)",
        resolver
            .config()
            .name_servers()
            .iter()
            .map(|ns| ns.socket_addr)
            .collect::<HashSet<SocketAddr>>()
    );

    let mut query = format!("{}.user._bitcoin-payment.{}", user, domain);

    'outer: loop {
        let txt_response = resolver.lookup(query.clone(), RecordType::TXT).await?;
        log::debug!("{:?}", txt_response);

        let mut bip21_result = None;

        for proven_rdata in txt_response.dnssec_iter() {
            let (proof, rdata) = proven_rdata.into_parts();
            if !proof.is_secure() {
                continue;
            }

            if let Some(txt_type) = rdata.as_txt() {
                let txt = txt_type
                    .iter()
                    .map(|b| String::from_utf8_lossy(b))
                    .collect::<Vec<_>>()
                    .join("");

                if !txt.starts_with("bitcoin:") {
                    continue;
                }
                if let Some(_bip21) = bip21_result {
                    return Err(anyhow!("multiple bip21 entries found in txt records!"));
                }
                bip21_result = Some(txt.split_once(":").unwrap().1.to_owned())
            }
        }

        if let Some(bip21) = bip21_result {
            for bip21_param in bip21.split("?") {
                if bip21_param.starts_with("lno=") {
                    let offer = bip21_param.strip_prefix("lno=").unwrap().to_owned();
                    log::debug!("bip353 offer: {}", offer);
                    return fetch_invoice_bolt12(
                        plugin,
                        invstring_name,
                        offer,
                        Some(amount_msat),
                        message,
                        params,
                    )
                    .await;
                }
            }
            return Err(anyhow!("no offer found in txt dns entry"));
        }

        let cname_response = resolver.lookup(query.clone(), RecordType::CNAME).await?;

        for proven_rdata in cname_response.dnssec_iter() {
            let (proof, rdata) = proven_rdata.into_parts();
            if !proof.is_secure() {
                continue;
            }
            if let Some(cname_type) = rdata.as_cname() {
                query = cname_type.to_string();
                log::debug!("CNAME found, redirecting to: {}", query);
                continue 'outer;
            }
        }
        break;
    }

    Err(anyhow!(
        "bip353 offer not found or DNSSEC signatures not secure"
    ))
}
