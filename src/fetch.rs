use anyhow::{anyhow, Error};
use cln_plugin::Plugin;
use cln_rpc::primitives::Amount;
use serde_json::Map;

use crate::{
    hooks::Paycmd,
    lnurl::{lnurl_fetch_invoice, resolve_lnurl},
    offer::{bip353_flow, fetch_offer_inv},
};

pub async fn resolve_invstring(
    plugin: Plugin<()>,
    params: &mut Map<String, serde_json::Value>,
    paycmd: Paycmd,
) -> Result<Map<String, serde_json::Value>, Error> {
    let invstring_name = match paycmd {
        Paycmd::Pay => "bolt11",
        Paycmd::Xpay => "invstring",
    };
    let invstring_lower_presplit = if let Some(invstr) = params.get(invstring_name) {
        invstr.as_str().unwrap().to_string().to_lowercase()
    } else {
        return Err(anyhow!("missing {} param", invstring_name));
    };
    let invstring_lower = if let Some((_hrp, invstr)) = invstring_lower_presplit.split_once(":") {
        invstr.to_string()
    } else {
        invstring_lower_presplit
    };
    let amount_msat = params
        .get("amount_msat")
        .map(|amt| Amount::from_msat(amt.as_u64().unwrap()));
    let message = params
        .get("message")
        .map(|msg| msg.as_str().unwrap().to_string());

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
        return fetch_ln_address(
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
        if amount_msat.is_none() {
            return Err(anyhow!("offer: missing amount_msat"));
        }
        return fetch_offer_inv(
            plugin,
            invstring_name,
            invstring_lower,
            amount_msat.unwrap(),
            message,
            params,
        )
        .await;
    } else {
        log::debug!("regular invoice forwarded");
        return Ok(params.clone());
    }
}

async fn fetch_ln_address(
    plugin: Plugin<()>,
    invstring_name: &str,
    lnaddress: String,
    amount: Amount,
    message: Option<String>,
    params: &mut Map<String, serde_json::Value>,
) -> Result<Map<String, serde_json::Value>, Error> {
    let address_parts = lnaddress.split("@").collect::<Vec<&str>>();

    if address_parts.len() != 2 {
        return Err(anyhow!("LN-address invalid: {}", lnaddress));
    }

    let user = address_parts.first().unwrap();

    let domain = address_parts.get(1).unwrap();

    let bip353_error = match bip353_flow(
        plugin.clone(),
        invstring_name,
        user,
        domain,
        amount,
        message.clone(),
        params,
    )
    .await
    {
        Ok(bip353) => return Ok(bip353),
        Err(e) => e,
    };

    let ln_service_url = format!("https://{domain}/.well-known/lnurlp/{user}");

    match lnurl_fetch_invoice(
        plugin,
        invstring_name,
        ln_service_url,
        Some(lnaddress),
        amount,
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
