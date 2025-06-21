use std::{path::Path, time::Duration};

use anyhow::{anyhow, Context, Error};
use cln_plugin::Plugin;
use cln_rpc::{
    model::requests::DecodeRequest,
    primitives::{Amount, Sha256},
    ClnRpc,
};
use serde_json::Map;

use crate::structs::{LnurlpCallback, LnurlpConfig, PluginState};

pub async fn fetch_invoice_lnurl(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    config_url: String,
    lnaddress: Option<String>,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let config = plugin.state().config.lock().clone();

    let client = if let Some(tp) = config.tor_proxy {
        let proxy = reqwest::Proxy::all(format!("socks5h://{}", tp))?;
        reqwest::Client::builder()
            .proxy(proxy)
            .timeout(Duration::from_secs(30))
            .build()?
    } else {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?
    };
    let lnurlp_config_raw = client.get(config_url).send().await?;
    if !lnurlp_config_raw.status().is_success() {
        return Err(anyhow!(
            "LNURL: got bad status for lnurl config: {}",
            lnurlp_config_raw.status()
        ));
    }
    log::debug!("lnurl config: {:?}", lnurlp_config_raw);
    let lnurlp_config = lnurlp_config_raw
        .json::<LnurlpConfig>()
        .await
        .context("Not a valid LNURL config response")?;

    validate_lnurl_config(&lnurlp_config, amount_msat, lnaddress, config.strict_lnurl)?;

    let mut callback_url = format!("{}?amount={}", lnurlp_config.callback, amount_msat.msat());
    if let Some(msg) = message {
        let comment_length = lnurlp_config
            .comment_allowed
            .ok_or_else(|| anyhow!("LNURL: message not supported for this address!"))?;
        if comment_length >= (msg.len() as u64) {
            callback_url += &format!("&comment={}", msg);
        } else {
            return Err(anyhow!(
                "LNURL: message too long for this address! {}>{}",
                msg.len(),
                comment_length
            ));
        }
    }
    let callback_response_raw = client.get(callback_url).send().await?;
    if !callback_response_raw.status().is_success() {
        return Err(anyhow!(
            "LNURL: got bad status for invoice: {}",
            callback_response_raw.status()
        ));
    }
    let callback_response = callback_response_raw.json::<LnurlpCallback>().await?;

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
        if config.strict_lnurl {
            return Err(anyhow!("Strict mode: Lnurl: missing description hash!"));
        } else {
            // Some servers are not including a description hash
            log::info!(
                "Lnurl: missing description hash, please report to lnaddress \
                service provider they are violating the spec in LUD-06"
            );
        }
    } else {
        let metadata_hashed = Sha256::const_hash(lnurlp_config.metadata.as_bytes());
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
    Ok(())
}

fn validate_lnurl_config(
    lnurl_config: &LnurlpConfig,
    amount_msat: Amount,
    lnaddress: Option<String>,
    strict_lnurl: bool,
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
            if strict_lnurl {
                return Err(anyhow!(
                    "Strict mode: Lnaddress not found in metadata!: {}",
                    lnurl_config.metadata
                ));
            } else {
                log::info!(
                    "Lnaddress not found in metadata, please report to lnaddress \
                service provider they are violating the spec in LUD-16"
                );
            }
        }
    }

    Ok(())
}

pub async fn resolve_lnurl(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    invstring: String,
    lnaddress: Option<String>,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let (hrp, config_url_bytes) = bech32::decode(&invstring)?;
    let config_url = String::from_utf8(config_url_bytes)?;
    log::debug!("lnurl hrp:{} url:{}", hrp, config_url);

    fetch_invoice_lnurl(
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
