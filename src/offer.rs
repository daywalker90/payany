use std::path::Path;

use anyhow::{anyhow, Error};
use cln_plugin::Plugin;
use cln_rpc::{
    model::requests::{DecodeRequest, FetchinvoiceRequest},
    primitives::Amount,
    ClnRpc,
};
use hickory_resolver::{
    config::{ResolverConfig, ResolverOpts},
    lookup::Lookup,
    proto::{dnssec, rr::RecordType},
    Resolver,
};
use serde_json::Map;

pub async fn fetch_offer_inv(
    plugin: Plugin<()>,
    invstring_name: &str,
    invstring: String,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<Map<String, serde_json::Value>, Error> {
    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let offer_decoded = rpc
        .call_typed(&DecodeRequest {
            string: invstring.clone(),
        })
        .await?;

    if offer_decoded.offer_amount_msat.is_some()
        && offer_decoded.offer_amount_msat.unwrap().msat() != amount_msat.msat()
    {
        return Err(anyhow!(
            "offer: not matching stated amount_msat: {} != {}",
            offer_decoded.offer_amount_msat.unwrap().msat(),
            amount_msat.msat()
        ));
    }

    let fetch_amount_msat = if offer_decoded.offer_amount_msat.is_some() {
        None
    } else {
        Some(amount_msat)
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
        })
        .await?;

    let invoice_decoded = rpc
        .call_typed(&DecodeRequest {
            string: invoice.invoice.clone(),
        })
        .await?;

    if invoice_decoded.invoice_amount_msat.unwrap().msat() != amount_msat.msat() {
        return Err(anyhow!(
            "offers: got invoice with different amount_msat!: {} != {}",
            invoice_decoded.invoice_amount_msat.unwrap().msat(),
            amount_msat.msat()
        ));
    }

    params.remove("amount_msat");
    *params.get_mut(invstring_name).unwrap() = serde_json::Value::String(invoice.invoice);
    Ok(params.clone())
}

pub async fn bip353_flow(
    plugin: Plugin<()>,
    invstring_name: &str,
    user: &str,
    domain: &str,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<Map<String, serde_json::Value>, Error> {
    let mut resolver_opts = ResolverOpts::default();
    resolver_opts.validate = true;
    let resolver = Resolver::tokio(ResolverConfig::default(), resolver_opts);

    let mut query = format!("{}.user._bitcoin-payment.{}", user, domain);

    'outer: loop {
        let lookup_response = resolver.lookup(query.clone(), RecordType::ANY).await?;

        let mut bip21_found = None;

        if !good_rrsig(&lookup_response) {
            return Err(anyhow!("DNSSEC signature is not secure!"));
        };

        for proven_rdata in lookup_response.dnssec_iter() {
            let (proof, rdata) = proven_rdata.into_parts();
            if !proof.is_secure() {
                continue;
            }

            if let Some(txt_type) = rdata.as_txt() {
                for txt_bytes in txt_type.iter() {
                    let txt = if let Ok(txt_str) = String::from_utf8(txt_bytes.clone().into_vec()) {
                        txt_str
                    } else {
                        continue;
                    };
                    if !txt.starts_with("bitcoin:") {
                        continue;
                    }
                    if let Some(_bip21) = bip21_found {
                        return Err(anyhow!("multiple bip21 entries found in txt records!"));
                    }
                    bip21_found = Some(txt.split_once(":").unwrap().1.to_string())
                }
            }
        }

        if let Some(bip21) = bip21_found {
            for bip21_param in bip21.split("?") {
                if bip21_param.starts_with("lno=") {
                    let offer = bip21_param.strip_prefix("lno=").unwrap().to_string();
                    log::debug!("bip353 offer: {}", offer);
                    return fetch_offer_inv(
                        plugin,
                        invstring_name,
                        offer,
                        amount_msat,
                        message,
                        params,
                    )
                    .await;
                }
            }
            return Err(anyhow!("no offer found in txt dns entry"));
        }

        for proven_rdata in lookup_response.dnssec_iter() {
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

    Err(anyhow!("bip353 offer not found"))
}

fn good_rrsig(lookup_response: &Lookup) -> bool {
    for record in lookup_response.iter() {
        if let Some(dnssec_type) = record.as_dnssec() {
            if let Some(rrsig_type) = dnssec_type.as_rrsig() {
                match rrsig_type.algorithm() {
                    dnssec::Algorithm::RSASHA256 => return rrsig_type.sig().len() >= 128,
                    dnssec::Algorithm::RSASHA512 => return rrsig_type.sig().len() >= 128,
                    dnssec::Algorithm::ECDSAP256SHA256 => return true,
                    dnssec::Algorithm::ECDSAP384SHA384 => return true,
                    dnssec::Algorithm::ED25519 => return true,
                    _ => return false,
                };
            };
        };
    }
    false
}
