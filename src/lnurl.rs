use std::path::Path;

use anyhow::{anyhow, Context, Error};
use cln_plugin::Plugin;
use cln_rpc::{
    model::requests::DecodeRequest,
    primitives::{Amount, Sha256},
    ClnRpc,
};
use serde::{Deserialize, Serialize};
use serde_json::Map;

#[derive(Debug, Serialize, Deserialize)]
struct LnurlpConfig {
    callback: String,
    #[serde(rename = "maxSendable")]
    max_sendable: u64,
    #[serde(rename = "minSendable")]
    min_sendable: u64,
    metadata: String,
    tag: String,
    #[serde(rename = "commentAllowed")]
    comment_allowed: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LnurlpCallback {
    pr: String,
    routes: Vec<String>,
}

pub async fn lnurl_fetch_invoice(
    plugin: Plugin<()>,
    invstring_name: &str,
    config_url: String,
    lnaddress: Option<String>,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<Map<String, serde_json::Value>, Error> {
    let client = reqwest::Client::new();
    let lnurl_config = client
        .get(config_url)
        .send()
        .await?
        .json::<LnurlpConfig>()
        .await
        .context("Not a valid LNURL config response")?;
    log::debug!("lnurl config: {:?}", lnurl_config);

    validate_lnurl_config(&lnurl_config, amount_msat, lnaddress)?;

    let mut callback_url = format!("{}?amount={}", lnurl_config.callback, amount_msat.msat());
    if let Some(msg) = message {
        if let Some(cmt) = lnurl_config.comment_allowed {
            if cmt >= (msg.len() as u64) {
                callback_url += &format!("&comment={}", msg);
            } else {
                return Err(anyhow!(
                    "LNURL: message too long for this address! {}>{}",
                    msg.len(),
                    cmt
                ));
            }
        } else {
            return Err(anyhow!("LNURL: message not supported for this address!"));
        }
    }
    let callback_response = client
        .get(callback_url)
        .send()
        .await?
        .json::<LnurlpCallback>()
        .await?;

    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let invoice_decoded = rpc
        .call_typed(&DecodeRequest {
            string: callback_response.pr.clone(),
        })
        .await?;
    if invoice_decoded.amount_msat.is_none() || invoice_decoded.amount_msat.unwrap() != amount_msat
    {
        return Err(anyhow!(
            "Lnurl: wrong amount in invoice: {}!={}",
            invoice_decoded.amount_msat.map(|a| a.msat()).unwrap_or(0),
            amount_msat.msat()
        ));
    }
    if invoice_decoded.description_hash.is_none() {
        // Some servers are not including a description hash
        log::info!(
            "Lnurl: missing description hash, please report to lnaddress \
            service provider they are violating the spec in LUD-06"
        );
        // return Err(anyhow!("Lnurl: missing description hash!"));
    } else {
        let metadata_hashed = Sha256::const_hash(lnurl_config.metadata.as_bytes());
        log::debug!(
            "Lnurl: metadata_hashed:{} description_hash:{}",
            metadata_hashed,
            invoice_decoded.description_hash.unwrap()
        );
        if invoice_decoded.description_hash.unwrap() != metadata_hashed {
            return Err(anyhow!(
                "Lnurl: description hash not matching metadata! {} != {}",
                metadata_hashed,
                invoice_decoded.description_hash.unwrap()
            ));
        }
    }

    params.remove("amount_msat");
    *params.get_mut(invstring_name).unwrap() = serde_json::Value::String(callback_response.pr);
    Ok(params.clone())
}

fn validate_lnurl_config(
    lnurl_config: &LnurlpConfig,
    amount_msat: Amount,
    lnaddress: Option<String>,
) -> Result<(), Error> {
    if !lnurl_config.tag.eq_ignore_ascii_case("payRequest") {
        return Err(anyhow!(
            "LNURL config is not for a payRequest: {}",
            lnurl_config.tag
        ));
    }

    if amount_msat.msat() < lnurl_config.min_sendable {
        return Err(anyhow!(
            "Amount is below minimum sendable! {}<{}",
            amount_msat.msat(),
            lnurl_config.min_sendable
        ));
    }
    if amount_msat.msat() > lnurl_config.max_sendable {
        return Err(anyhow!(
            "Amount is above maximum sendable! {}>{}",
            amount_msat.msat(),
            lnurl_config.max_sendable
        ));
    }
    if let Some(lnaddr) = lnaddress {
        let metadata_json: serde_json::Value = serde_json::from_str(&lnurl_config.metadata)?;
        let mut lnaddress_found = false;
        let metadata_outer_array = if let serde_json::Value::Array(meta_arr) = metadata_json {
            meta_arr
        } else {
            return Err(anyhow!("metadata not an array!: {}", lnurl_config.metadata));
        };

        for meta in metadata_outer_array {
            let metadata_inner_array = if let serde_json::Value::Array(meta_inners) = meta {
                meta_inners
            } else {
                return Err(anyhow!("inner metadata not an array!: {}", &meta));
            };
            if metadata_inner_array.len() != 2 {
                return Err(anyhow!(
                    "inner metadata array is not of length 2!: {:?}",
                    metadata_inner_array
                ));
            }
            let data_type = metadata_inner_array
                .first()
                .unwrap()
                .as_str()
                .ok_or(anyhow!("inner metadata identifier is not a string:"))?;
            if !data_type.eq_ignore_ascii_case("text/identifier")
                && !data_type.eq_ignore_ascii_case("text/email")
            {
                continue;
            }
            let data = metadata_inner_array
                .get(1)
                .unwrap()
                .as_str()
                .ok_or(anyhow!("inner metadata content is not a string:"))?;
            if data.eq_ignore_ascii_case(&lnaddr) {
                lnaddress_found = true;
            }
        }

        // Quite a few servers in the wild are not including the text/identifier or text/email data..
        if !lnaddress_found {
            log::info!(
                "Lnaddress not found in metadata, please report to lnaddress \
            service provider they are violating the spec in LUD-16"
            );
            // return Err(anyhow!(
            //     "Lnaddress not found in metadata!: {}",
            //     lnurl_config.metadata
            // ));
        }
    }

    Ok(())
}

pub async fn resolve_lnurl(
    plugin: Plugin<()>,
    invstring_name: &str,
    invstring: String,
    lnaddress: Option<String>,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<Map<String, serde_json::Value>, Error> {
    let (hrp, config_url_bytes) = bech32::decode(&invstring)?;
    let config_url = String::from_utf8(config_url_bytes)?;
    log::debug!("lnurl hrp:{} url:{}", hrp, config_url);

    lnurl_fetch_invoice(
        plugin,
        invstring_name,
        config_url,
        lnaddress,
        amount_msat,
        message,
        params,
    )
    .await
}
