use anyhow::{Error, anyhow};
use cln_plugin::Plugin;
use cln_rpc::RpcError;
use serde_json::{Map, json};

use crate::{
    budget::budget_check,
    fetch::resolve_invstring,
    parse::convert_pay_to_xpay,
    structs::{Config, ParamValue, Paycmd, PluginState, RpcCommand},
};

pub async fn hook_handler(
    plugin: Plugin<PluginState>,
    args: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let root: RpcCommand = match serde_json::from_value(args.clone()) {
        Ok(o) => o,
        Err(e) => {
            log::debug!("Could not deserialize rpc_command: {e}");
            return Ok(json!({"result":"continue"}));
        }
    };
    let mut paycmd = match root.rpc_command.method.as_str() {
        "xpay" => Paycmd::Xpay,
        "pay" => Paycmd::Pay,
        "renepay" => Paycmd::Renepay,
        "setconfig" => {
            if let Err(e) = check_setconfig(root.rpc_command.params) {
                return Ok(json!({"return":{"error":json!(RpcError {
                    code: Some(-32602),
                    message: e.to_string(),
                    data: None,
                })}}));
            }
            return Ok(json!({"result":"continue"}));
        }
        _ => return Ok(json!({"result":"continue"})),
    };

    log::debug!("params: {:?}", root.rpc_command.params);

    let config = plugin.state().config.lock().clone();

    if config.ignore_deprecated_pays {
        match paycmd {
            Paycmd::Xpay => (),
            Paycmd::Pay | Paycmd::Renepay => return Ok(json!({"result":"continue"})),
        }
    }

    let mut params_as_object = match root.rpc_command.params.to_object(paycmd, &config) {
        Ok(o) => o,
        Err(e) => {
            return Ok(json!({"return":{"error":json!(RpcError {
                code: Some(-32602),
                message: e.to_string(),
                data: None,
            })}}));
        }
    };
    log::debug!("params_obj: {params_as_object:?}");

    match resolve_invstring(plugin.clone(), &mut params_as_object).await {
        Ok(o) => o,
        Err(e) => {
            params_as_object.remove("message");
            return Ok(json!({"return": {"error":json!(RpcError {
                code: Some(-32602),
                message: format!("payany could not fetch invoice: {e}"),
                data: None,
            })}}));
        }
    }

    if let Err(e) = handle_lno_message(paycmd, &config, &mut params_as_object) {
        return Ok(json!({"return":{"error":json!(RpcError {
            code: Some(-32602),
            message: format!("payany: {e}"),
            data: None,
        })}}));
    }
    params_as_object.remove("message");

    if let Err(e) = budget_check(plugin.clone(), &params_as_object, paycmd).await {
        return Ok(json!({"return": {"error":json!(RpcError {
            code: Some(-32602),
            message: format!("payany budget exceeded: {e}"),
            data: None,
        })}}));
    }

    if config.xpay_handle_pay && paycmd == Paycmd::Pay {
        if let Err(e) = convert_pay_to_xpay(plugin.clone(), &mut params_as_object).await {
            return Ok(json!({"return": {"error":json!(RpcError {
                code: Some(-32602),
                message: format!("payany conversion to xpay failed: {e}"),
                data: None,
            })}}));
        }
        paycmd = Paycmd::Xpay;
    }

    let result = json!({"replace": {"jsonrpc":"2.0",
    "id": root.rpc_command.id,
    "method":format!("{}",match paycmd{
        Paycmd::Pay => "pay",
        Paycmd::Xpay => "xpay",
        Paycmd::Renepay=> "renepay"
    }),
    "params":params_as_object}});
    log::debug!("{result}");
    Ok(result)
}

fn handle_lno_message(
    paycmd: Paycmd,
    config: &Config,
    params: &mut Map<String, serde_json::Value>,
) -> Result<(), anyhow::Error> {
    let invstring = params
        .get("invstring")
        .or_else(|| params.get("bolt11"))
        .and_then(|v| v.as_str());
    if invstring.is_none() || !invstring.unwrap().to_lowercase().starts_with("lno") {
        return Ok(());
    }
    if !params.contains_key("message") {
        return Ok(());
    }
    if params.contains_key("payer_note") {
        return Err(anyhow!("cannot set both message and payer_note"));
    }
    let args = match paycmd {
        Paycmd::Pay => &config.payargs,
        Paycmd::Xpay => &config.xpayargs,
        Paycmd::Renepay => &config.renepayargs,
    };
    if !args.iter().any(|a| a.eq("payer_note")) {
        return Err(anyhow!(
            "message for offers requires the `payer_note` argument, upgrade CLN to v26.04 and \
            use `xpay` to support it"
        ));
    }
    let message = params.remove("message").unwrap();
    params.insert("payer_note".to_owned(), message);
    Ok(())
}

fn check_setconfig(param_val: ParamValue) -> Result<(), anyhow::Error> {
    let config;
    let mut val = None;
    match param_val {
        ParamValue::Array(values) => {
            if let Some(f) = values.first() {
                config = Some(f.as_str().map(std::borrow::ToOwned::to_owned)).flatten();
            } else {
                return Ok(());
            }
            if let Some(v) = values.get(1) {
                val = Some(v.clone());
            } else {
                return Ok(());
            }
        }
        ParamValue::Object(map) => {
            config = map
                .get("config")
                .and_then(|s| s.as_str().map(std::borrow::ToOwned::to_owned));
            val = map.get("val").cloned();
        }
        ParamValue::String(s) => config = Some(s),
    }
    let Some(config) = config else {
        return Ok(());
    };
    if config.eq_ignore_ascii_case("xpay-handle-pay") {
        let val = val.ok_or_else(|| {
            anyhow!("Setting xpay-handle-pay to true when payany is active is blocked")
        })?;

        if let Some(v_b) = val.as_bool() {
            if v_b {
                return Err(anyhow!(
                    "Setting xpay-handle-pay to true when payany is active is blocked"
                ));
            }
            return Ok(());
        } else if let Some(s) = val.as_str() {
            if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("1") {
                return Err(anyhow!(
                    "Setting xpay-handle-pay to true when payany is active is blocked"
                ));
            }
            return Ok(());
        }
    }

    Ok(())
}
