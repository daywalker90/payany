use std::{path::Path, time::Duration};

use anyhow::{Context, Error, anyhow};
use bitreq::Url;
use cln_plugin::Plugin;
use cln_rpc::{ClnRpc, model::requests::DecodeRequest, primitives::Amount};
use serde_json::Map;

use crate::structs::{Config, LnurlpCallback, LnurlpConfig, PluginState};

fn is_lud01_url(url: &bitreq::Url) -> bool {
    let scheme = url.scheme();
    if scheme == "https" {
        return true;
    }
    if scheme != "http" {
        return false;
    }
    let host = url.base_url();
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host.ends_with(".onion")
}

pub async fn try_fetch_lnurl(
    config: &Config,
    lnaddress: Option<&str>,
    config_url: String,
    amount_msat: Amount,
    message: Option<String>,
) -> Result<LnurlpCallback, Error> {
    let mut request_config = bitreq::get(config_url).with_timeout(Duration::from_secs(30));
    if let Some(proxy) = &config.tor_proxy {
        request_config = request_config.with_proxy(proxy.clone());
    }
    let lnurlp_config_raw = match request_config.send_async().await {
        Ok(o) => o,
        Err(e) => {
            log::warn!("LNURL: failed to fetch lnurl config: {e:?}");
            return Err(anyhow!(e));
        }
    };
    if lnurlp_config_raw.status_code < 200 || lnurlp_config_raw.status_code > 299 {
        return Err(anyhow!(
            "LNURL: got bad status for lnurl config: {}",
            lnurlp_config_raw.status_code
        ));
    }
    log::debug!("lnurl config: {lnurlp_config_raw:?}");
    let lnurlp_config = lnurlp_config_raw
        .json::<LnurlpConfig>()
        .context("Not a valid LNURL config response")?;

    validate_lnurl_config(&lnurlp_config, amount_msat, lnaddress, config.strict_lnurl)?;

    let mut callback_url = bitreq::Url::parse(&lnurlp_config.callback)?;
    if !is_lud01_url(&callback_url) {
        return Err(anyhow!(
            "LNURL callback must be an https:// clearnet link or an http:// onion link, got {}",
            lnurlp_config.callback
        ));
    }
    {
        callback_url.append_query_params([("amount", amount_msat.msat().to_string().as_str())]);
        if let Some(msg) = message {
            let comment_length = lnurlp_config
                .comment_allowed
                .ok_or_else(|| anyhow!("LNURL: message not supported for this address!"))?;
            if comment_length >= msg.chars().count() as u64 {
                callback_url.append_query_params([("comment", msg.as_str())]);
            } else {
                return Err(anyhow!(
                    "LNURL: message too long for this address! {}>{}",
                    msg.chars().count(),
                    comment_length
                ));
            }
        }
    }

    let mut request_callback =
        bitreq::get(callback_url.as_str()).with_timeout(Duration::from_secs(30));
    if let Some(proxy) = &config.tor_proxy {
        request_callback = request_callback.with_proxy(proxy.clone());
    }
    let callback_response_raw = request_callback.send_async().await?;
    if callback_response_raw.status_code < 200 || callback_response_raw.status_code > 299 {
        return Err(anyhow!(
            "LNURL: got bad status for invoice: {}",
            callback_response_raw.status_code
        ));
    }
    let callback_response = callback_response_raw.json::<LnurlpCallback>()?;
    Ok(callback_response)
}

pub async fn process_lnurl_invoice(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    callback_response: LnurlpCallback,
    amount_msat: Amount,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
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
            invoice_decoded.amount_msat.map_or(0, |a| a.msat()),
            amount_msat.msat()
        ));
    }

    params.remove("amount_msat");
    *params.get_mut(invstring_name).unwrap() = serde_json::Value::String(callback_response.pr);
    Ok(())
}

fn validate_lnurl_config(
    lnurl_config: &LnurlpConfig,
    amount_msat: Amount,
    lnaddress: Option<&str>,
    strict_lnurl: bool,
) -> Result<(), Error> {
    if !lnurl_config.tag.eq_ignore_ascii_case("payRequest") {
        return Err(anyhow!(
            "LNURL config is not for a payRequest: {}",
            lnurl_config.tag
        ));
    }

    if lnurl_config.min_sendable > lnurl_config.max_sendable {
        return Err(anyhow!(
            "minSendable {} cannot be more than maxSendable {}",
            lnurl_config.min_sendable,
            lnurl_config.max_sendable
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
        let serde_json::Value::Array(metadata_outer_array) = metadata_json else {
            return Err(anyhow!("metadata not an array!: {}", lnurl_config.metadata));
        };

        for meta in metadata_outer_array {
            let serde_json::Value::Array(metadata_inner_array) = meta else {
                return Err(anyhow!("inner metadata not an array!: {meta}"));
            };
            let mut inner = metadata_inner_array.into_iter();
            let data_type = inner
                .next()
                .ok_or(anyhow!("inner metadata array is empty!"))?
                .as_str()
                .ok_or(anyhow!("inner metadata identifier is not a string:"))?
                .to_owned();
            let data = inner
                .next()
                .ok_or(anyhow!("inner metadata array has no data!"))?;
            if data_type.eq_ignore_ascii_case("text/identifier")
                || data_type.eq_ignore_ascii_case("text/email")
            {
                let data = data
                    .as_str()
                    .ok_or(anyhow!("inner metadata content is not a string:"))?;
                if data.eq_ignore_ascii_case(lnaddr) {
                    lnaddress_found = true;
                }
            }
        }

        // Quite a few servers in the wild are not including the text/identifier or text/email data..
        if !lnaddress_found {
            if strict_lnurl {
                return Err(anyhow!(
                    "Strict mode: Lnaddress not found in metadata!: {}",
                    lnurl_config.metadata
                ));
            }
            log::info!(
                "Lnaddress not found in metadata, please report to lnaddress \
            service provider they are violating the spec in LUD-16"
            );
        }
    }

    Ok(())
}

pub async fn resolve_lnurl(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    invstring: &str,
    lnaddress: Option<&str>,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let (hrp, config_url_bytes) = bech32::decode(invstring)?;
    let config_url = String::from_utf8(config_url_bytes)?;
    log::debug!("lnurl hrp:{hrp} url:{config_url}");

    let parsed = Url::parse(&config_url)?;
    if !is_lud01_url(&parsed) {
        return Err(anyhow!(
            "LNURL must be an https:// clearnet link or an http:// onion link, got {config_url}"
        ));
    }

    let config = plugin.state().config.lock().clone();

    let lnurlp_callback =
        try_fetch_lnurl(&config, lnaddress, config_url, amount_msat, message).await?;

    process_lnurl_invoice(plugin, invstring_name, lnurlp_callback, amount_msat, params).await
}

#[cfg(test)]
mod tests {
    use super::is_lud01_url;

    #[test]
    fn test_is_lud01_url() {
        assert!(is_lud01_url(
            &bitreq::Url::parse("https://service.com/api?q=1").unwrap()
        ));
        assert!(is_lud01_url(
            &bitreq::Url::parse("https://sub.example.org/x").unwrap()
        ));
        assert!(is_lud01_url(
            &bitreq::Url::parse("http://abcdefghijklmnop.onion/x").unwrap()
        ));
        assert!(is_lud01_url(
            &bitreq::Url::parse("http://abcdefghijklmnop.onion:8080/x").unwrap()
        ));
        assert!(is_lud01_url(
            &bitreq::Url::parse("http://localhost:9737/x").unwrap()
        ));
        assert!(is_lud01_url(
            &bitreq::Url::parse("http://localhost./x").unwrap()
        ));
        assert!(is_lud01_url(
            &bitreq::Url::parse("http://127.0.0.1:8081/x").unwrap()
        ));
        assert!(is_lud01_url(
            &bitreq::Url::parse("http://[::1]:8080/x").unwrap()
        ));
        assert!(!is_lud01_url(
            &bitreq::Url::parse("http://service.com/x").unwrap()
        ));
        assert!(!is_lud01_url(
            &bitreq::Url::parse("http://service.com.onion.evil.com/x").unwrap()
        ));
        assert!(!is_lud01_url(
            &bitreq::Url::parse("http://notonion.com/x").unwrap()
        ));
        assert!(!is_lud01_url(
            &bitreq::Url::parse("http://mylocalhost.com/x").unwrap()
        ));
        assert!(!is_lud01_url(
            &bitreq::Url::parse("http://127.0.0.1.evil.com/x").unwrap()
        ));
        assert!(!is_lud01_url(
            &bitreq::Url::parse("ftp://service.com/x").unwrap()
        ));
    }
}
