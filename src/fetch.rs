use anyhow::{anyhow, Error};
use cln_plugin::Plugin;
use cln_rpc::primitives::Amount;
use serde_json::Map;

use crate::{
    lnurl::{fetch_invoice_lnurl, resolve_lnurl},
    offer::{fetch_invoice_bip353, fetch_invoice_bolt12},
    structs::{Paycmd, PluginState, URI_SCHEMES},
};

pub async fn resolve_invstring(
    plugin: Plugin<PluginState>,
    params: &mut Map<String, serde_json::Value>,
    paycmd: Paycmd,
) -> Result<(), Error> {
    let invstring_name = match paycmd {
        Paycmd::Pay => "bolt11",
        Paycmd::Xpay | Paycmd::Renepay => "invstring",
    };
    let invstring_lower_presplit = if let Some(invstr) = params.get(invstring_name) {
        invstr
            .as_str()
            .ok_or_else(|| anyhow!("`invstring` must be a string"))?
            .to_owned()
            .to_lowercase()
    } else {
        return Err(anyhow!("missing required parameter: {}", invstring_name));
    };
    let mut invstring_lower = invstring_lower_presplit;
    for uri_scheme in URI_SCHEMES {
        if let Some(stripped) = invstring_lower.strip_prefix(uri_scheme) {
            invstring_lower = stripped.to_owned();
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
    } else if invstring_lower.contains("@") {
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
        log::debug!("bolt12 offer detected");
        return fetch_invoice_bolt12(
            plugin,
            invstring_name,
            invstring_lower,
            amount_msat,
            message,
            params,
        )
        .await;
    } else {
        log::debug!("regular invoice forwarded");
        return Ok(());
    }
}

async fn resolve_lnaddress(
    plugin: Plugin<PluginState>,
    invstring_name: &str,
    lnaddress: String,
    amount_msat: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let address_parts = lnaddress.split("@").collect::<Vec<&str>>();

    if address_parts.len() != 2 {
        return Err(anyhow!("LN-address invalid: {}", lnaddress));
    }

    let user = address_parts.first().unwrap();

    let domain = address_parts.get(1).unwrap();

    let bip353_error = match fetch_invoice_bip353(
        plugin.clone(),
        invstring_name,
        user,
        domain,
        amount_msat,
        message.clone(),
        params,
    )
    .await
    {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };

    let ln_service_url = if domain.contains("localhost") || domain.contains("127.0.0.1") {
        format!("http://{domain}/.well-known/lnurlp/{user}")
    } else {
        format!("https://{domain}/.well-known/lnurlp/{user}")
    };

    match fetch_invoice_lnurl(
        plugin,
        invstring_name,
        ln_service_url,
        Some(lnaddress),
        amount_msat,
        message,
        params,
    )
    .await
    {
        Ok(lnurl) => Ok(lnurl),
        Err(lnurl_error) => Err(anyhow!(
            "Error fetching invoice from bip353:{} and error fetching \
                    invoice from lnurl: {}",
            bip353_error,
            lnurl_error
        )),
    }
}
