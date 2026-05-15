use anyhow::{Error, anyhow};
use cln_plugin::Plugin;
use cln_rpc::primitives::Amount;
use serde_json::Map;

use crate::{
    lnurl::{process_lnurl_invoice, resolve_lnurl, try_fetch_lnurl},
    structs::{PluginState, URI_SCHEMES},
};

pub async fn resolve_invstring(
    plugin: Plugin<PluginState>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let invstring_name = if params.get("invstring").is_some() {
        "invstring"
    } else if params.get("bolt11").is_some() {
        "bolt11"
    } else {
        return Err(anyhow!("missing required parameter: `invstring`/`bolt11`"));
    };
    let invstring_lower_presplit = if let Some(invstr) = params.get(invstring_name) {
        invstr
            .as_str()
            .ok_or_else(|| anyhow!("{invstring_name} must be a string: {invstr}"))?
            .to_owned()
            .to_lowercase()
    } else {
        return Err(anyhow!("missing required parameter: {invstring_name}"));
    };
    let mut invstring_lower = invstring_lower_presplit.as_str();
    for uri_scheme in URI_SCHEMES {
        if let Some(stripped) = invstring_lower.strip_prefix(uri_scheme) {
            invstring_lower = stripped;
            break;
        }
    }
    let amount_msat = if let Some(amt) = params.get("amount_msat") {
        Some(Amount::from_msat(amt.as_u64().ok_or_else(|| {
            anyhow!("`amount_msat` must be an integer")
        })?))
    } else {
        None
    };
    let message = if let Some(msg) = params.get("message") {
        match msg {
            serde_json::Value::Number(number) => Some(number.to_string()),
            serde_json::Value::String(s) => Some(s.to_owned()),
            _ => return Err(anyhow!("`message` must be a string")),
        }
    } else {
        None
    };

    if invstring_lower.starts_with("lnurl") {
        log::debug!("lnurl detected");
        if amount_msat.is_none() {
            return Err(anyhow!("lnurl: missing amount_msat"));
        }
        return resolve_lnurl(
            plugin,
            invstring_name,
            invstring_lower,
            None,
            amount_msat.unwrap(),
            message,
            params,
        )
        .await;
    } else if invstring_lower.contains('@') {
        log::debug!("lnaddress detected");
        if amount_msat.is_none() {
            return Err(anyhow!("lnaddress: missing amount_msat"));
        }
        return resolve_lnaddress(
            plugin,
            invstring_name,
            invstring_lower,
            amount_msat.unwrap(),
            message,
            params,
        )
        .await;
    } else if invstring_lower.starts_with("lno") {
        log::debug!("regular bolt12 offer forwarded");
        return Ok(());
    }
    log::debug!("regular invoice forwarded");
    Ok(())
}

async fn resolve_lnaddress(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    lnaddress: &str,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let address_parts = lnaddress.split('@').collect::<Vec<&str>>();

    if address_parts.len() != 2 {
        return Err(anyhow!("LN-address invalid: {lnaddress}"));
    }

    let user = address_parts.first().unwrap();

    let domain = address_parts.get(1).unwrap();

    let ln_service_url = if domain.contains("localhost") || domain.contains("127.0.0.1") {
        format!("http://{domain}/.well-known/lnurlp/{user}")
    } else {
        format!("https://{domain}/.well-known/lnurlp/{user}")
    };

    let config = plugin.state().config.lock().clone();

    let (lnurlp_callback, lnurlp_config) = match try_fetch_lnurl(
        &config,
        Some(lnaddress),
        ln_service_url,
        amount_msat,
        message,
    )
    .await
    {
        Ok((cb, cf)) => (cb, cf),
        Err(e) => {
            log::info!("Error fetching lnurlp config: {e}, trying bip353 instead...");
            return Ok(());
        }
    };

    match process_lnurl_invoice(
        plugin,
        invstring_name,
        lnurlp_callback,
        lnurlp_config,
        amount_msat,
        &config,
        params,
    )
    .await
    {
        Ok(lnurl) => Ok(lnurl),
        Err(lnurl_error) => Err(anyhow!("Error fetching invoice from lnurl: {lnurl_error}")),
    }
}
