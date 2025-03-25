use anyhow::{anyhow, Error};
use cln_plugin::Plugin;
use cln_rpc::RpcError;
use serde_json::json;

use crate::{
    budget::budget_check,
    fetch::resolve_invstring,
    parse::convert_pay_to_xpay,
    structs::{ParamValue, Paycmd, PluginState, RpcCommand},
};

pub async fn hook_handler(
    plugin: Plugin<PluginState>,
    args: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let root: RpcCommand = match serde_json::from_value(args.clone()) {
        Ok(o) => o,
        Err(e) => {
            log::debug!("Could not deserialize rpc_command: {}", e);
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
            } else {
                return Ok(json!({"result":"continue"}));
            }
        }
        _ => return Ok(json!({"result":"continue"})),
    };

    log::debug!("params: {:?}", root.rpc_command.params);

    let config = plugin.state().config.lock().clone();
    let mut params_as_object = match root.rpc_command.params.to_object(paycmd, &config) {
        Ok(o) => o,
        Err(e) => {
            return Ok(json!({"return":{"error":json!(RpcError {
                code: Some(-32602),
                message: e.to_string(),
                data: None,
            })}}))
        }
    };
    log::debug!("params_obj: {:?}", params_as_object);

    match resolve_invstring(plugin.clone(), &mut params_as_object, paycmd).await {
        Ok(o) => o,
        Err(e) => {
            params_as_object.remove("message");
            return Ok(json!({"return": {"error":json!(RpcError {
                code: Some(-32602),
                message: format!("payany could not fetch invoice: {}", e),
                data: None,
            })}}));
        }
    };
    params_as_object.remove("message");

    if let Err(e) = budget_check(plugin.clone(), &params_as_object, paycmd).await {
        return Ok(json!({"return": {"error":json!(RpcError {
            code: Some(-32602),
            message: format!("payany budget exceeded: {}", e),
            data: None,
        })}}));
    }

    if config.xpay_handle_pay && paycmd == Paycmd::Pay {
        if let Err(e) = convert_pay_to_xpay(plugin.clone(), &mut params_as_object).await {
            return Ok(json!({"return": {"error":json!(RpcError {
                code: Some(-32602),
                message: format!("payany conversion to xpay failed: {}", e),
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
    log::debug!("{}", result);
    Ok(result)
}

fn check_setconfig(param_val: ParamValue) -> Result<(), anyhow::Error> {
    let config;
    let mut val = None;
    match param_val {
        ParamValue::Array(values) => {
            if let Some(f) = values.first() {
                config = Some(f.as_str().unwrap().to_owned())
            } else {
                return Ok(());
            }
            if let Some(v) = values.get(1) {
                val = Some(v.clone())
            } else {
                return Ok(());
            }
        }
        ParamValue::Object(map) => {
            config = map.get("config").map(|s| s.as_str().unwrap().to_owned());
            val = map.get("val").cloned()
        }
        ParamValue::String(s) => config = Some(s),
    };
    let config = if let Some(c) = config {
        c
    } else {
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
            } else {
                return Ok(());
            }
        } else if let Some(s) = val.as_str() {
            if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("1") {
                return Err(anyhow!(
                    "Setting xpay-handle-pay to true when payany is active is blocked"
                ));
            } else {
                return Ok(());
            }
        }
    }

    Ok(())
}
