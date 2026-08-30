use std::path::Path;

use anyhow::{Error, anyhow};
use cln_plugin::Plugin;
use cln_rpc::{
    ClnRpc,
    model::{
        requests::{DecodeRequest, FetchinvoiceRequest},
        responses::DecodeType,
    },
    primitives::Amount,
};
use serde_json::{Map, json};

use crate::{
    lnurl::{process_lnurl_invoice, resolve_lnurl, try_fetch_lnurl},
    structs::{Config, PluginState, URI_SCHEMES},
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

pub async fn resolve_offer_invoice(
    plugin: Plugin<PluginState>,
    config: &Config,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), Error> {
    let invstring_name = if params.contains_key("invstring") {
        "invstring"
    } else if params.contains_key("bolt11") {
        "bolt11"
    } else {
        return Ok(());
    };
    let Some(invstring) = params.get(invstring_name).and_then(|v| v.as_str()) else {
        return Ok(());
    };
    if !invstring.to_lowercase().starts_with("lno") {
        return Ok(());
    }
    if config.budget_amount_msat.is_none() || config.budget_per.is_none() {
        return Ok(());
    }

    let amount_msat = params
        .get("amount_msat")
        .and_then(serde_json::Value::as_u64);
    let payer_note = params
        .get("payer_note")
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let offer_decoded = rpc
        .call_typed(&DecodeRequest {
            string: invstring.to_owned(),
        })
        .await?;
    if offer_decoded.item_type != DecodeType::BOLT12_OFFER {
        return Err(anyhow!("Not a bolt12 offer: {invstring}"));
    }
    if let Some(currency) = offer_decoded.offer_currency {
        return Err(anyhow!(
            "Cannot pay offer in different currency: {currency}"
        ));
    }
    if offer_decoded.offer_recurrence.is_some() {
        return Err(anyhow!("Cannot pay recurring offers"));
    }
    let offer_amount_msat = offer_decoded.offer_amount_msat.map(|a| a.msat());
    let fetch_amount_msat = match offer_amount_msat {
        Some(offer_amt) => {
            if let Some(amt) = amount_msat {
                if amt < offer_amt {
                    return Err(anyhow!(
                        "Offer amount is {offer_amt}msat, amount_msat must be at least that,\
                         not {amt}"
                    ));
                }
                Some(amt)
            } else {
                None
            }
        }
        None => Some(amount_msat.ok_or_else(|| anyhow!("Must specify amount for this offer"))?),
    };

    let fetched = rpc
        .call_typed(&FetchinvoiceRequest {
            amount_msat: fetch_amount_msat.map(Amount::from_msat),
            bip353: None,
            payer_metadata: None,
            payer_note,
            quantity: None,
            recurrence_counter: None,
            recurrence_label: None,
            recurrence_start: None,
            timeout: None,
            offer: invstring.to_owned(),
        })
        .await?;

    let invoice_decoded = rpc
        .call_typed(&DecodeRequest {
            string: fetched.invoice.clone(),
        })
        .await?;
    if invoice_decoded.item_type != DecodeType::BOLT12_INVOICE {
        return Err(anyhow!("fetchinvoice did not return a bolt12 invoice"));
    }
    let invoice_amt = invoice_decoded
        .invoice_amount_msat
        .ok_or_else(|| anyhow!("Fetched invoice has no amount"))?
        .msat();
    let expected_msat = fetch_amount_msat
        .or(offer_amount_msat)
        .ok_or_else(|| anyhow!("Could not determine expected invoice amount"))?;
    if invoice_amt != expected_msat {
        return Err(anyhow!(
            "Invoice amount is {invoice_amt}, but expected {expected_msat}"
        ));
    }

    params.insert(invstring_name.to_owned(), json!(fetched.invoice));
    // The fetched invoice already carries its amount, and xpay rejects an
    // `amount_msat` on an amountful bolt12 invoice.
    params.remove("amount_msat");
    log::debug!("fetched bolt12 invoice for offer");
    Ok(())
}

fn lnurlp_base_url(domain: &str, user: &str) -> String {
    let host = if let Some(rest) = domain.strip_prefix('[') {
        match rest.find(']') {
            Some(end) => &domain[..end + 2],
            None => domain,
        }
    } else {
        domain.split(':').next().unwrap_or(domain)
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();

    let scheme = if host == "localhost"
        || host == "127.0.0.1"
        || host == "[::1]"
        || host.ends_with(".onion")
    {
        "http"
    } else {
        "https"
    };

    format!("{scheme}://{domain}/.well-known/lnurlp/{user}")
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

    let ln_service_url = lnurlp_base_url(domain, user);

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

#[cfg(test)]
mod tests {
    use super::lnurlp_base_url;

    #[test]
    fn test_local_hosts_use_http() {
        assert_eq!(
            lnurlp_base_url("localhost", "user"),
            "http://localhost/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("localhost:9737", "user"),
            "http://localhost:9737/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("127.0.0.1", "user"),
            "http://127.0.0.1/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("127.0.0.1:8081", "user"),
            "http://127.0.0.1:8081/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("[::1]:8080", "user"),
            "http://[::1]:8080/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("LOCALHOST.", "user"),
            "http://LOCALHOST./.well-known/lnurlp/user"
        );
    }

    #[test]
    fn test_lookalike_hosts_use_https() {
        assert_eq!(
            lnurlp_base_url("mylocalhost.com", "user"),
            "https://mylocalhost.com/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("localhost.com", "user"),
            "https://localhost.com/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("127.0.0.1.evil.com", "user"),
            "https://127.0.0.1.evil.com/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("2127.0.0.1", "user"),
            "https://2127.0.0.1/.well-known/lnurlp/user"
        );
    }

    #[test]
    fn test_onion_hosts_use_http() {
        assert_eq!(
            lnurlp_base_url("abcdefghijklmnop.onion", "user"),
            "http://abcdefghijklmnop.onion/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("abcdefghijklmnop.onion:8080", "user"),
            "http://abcdefghijklmnop.onion:8080/.well-known/lnurlp/user"
        );
        assert_eq!(
            lnurlp_base_url("notonion.com", "user"),
            "https://notonion.com/.well-known/lnurlp/user"
        );
    }
}
