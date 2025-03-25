use std::path::Path;

use anyhow::anyhow;
use cln_plugin::Plugin;
use cln_rpc::{model::requests::SetconfigRequest, ClnRpc};
use serde_json::json;

use crate::PluginState;

pub async fn check_handle_option(plugin: Plugin<PluginState>) -> Result<(), anyhow::Error> {
    let mut rpc = ClnRpc::new(
        Path::new(&plugin.configuration().lightning_dir).join(plugin.configuration().rpc_file),
    )
    .await?;

    let listconfigs: serde_json::Value = rpc
        .call_raw("listconfigs", &json!({"config":"xpay-handle-pay"}))
        .await?;

    let raw_configs = listconfigs
        .get("configs")
        .ok_or_else(|| anyhow!("no configs object"))?;
    let xpay_handle_pay = raw_configs
        .get("xpay-handle-pay")
        .ok_or_else(|| anyhow!("configs object missing xpay-handle-pay"))?;
    let value_bool = xpay_handle_pay
        .get("value_bool")
        .ok_or_else(|| anyhow!("no value_bool in xpay_handle_pay"))?
        .as_bool()
        .unwrap();

    if value_bool {
        if at_or_above_version(&plugin.state().config.lock().version, "25.02")? {
            rpc.call_typed(&SetconfigRequest {
                transient: Some(true),
                val: Some("false".to_owned()),
                config: "xpay-handle-pay".to_owned(),
            })
            .await?;
        } else {
            rpc.call_typed(&SetconfigRequest {
                transient: None,
                val: Some("false".to_owned()),
                config: "xpay-handle-pay".to_owned(),
            })
            .await?;
        }
        log::info!("Found activated `xpay-handle-pay`, `payany` deactivated it!")
    }

    Ok(())
}

pub fn at_or_above_version(my_version: &str, min_version: &str) -> Result<bool, anyhow::Error> {
    let clean_start_my_version = my_version
        .split_once('v')
        .ok_or_else(|| anyhow!("Could not find v in version string"))?
        .1;
    let full_clean_my_version: String = clean_start_my_version
        .chars()
        .take_while(|x| x.is_ascii_digit() || *x == '.')
        .collect();

    let my_version_parts: Vec<&str> = full_clean_my_version.split('.').collect();
    let min_version_parts: Vec<&str> = min_version.split('.').collect();

    if my_version_parts.len() <= 1 || my_version_parts.len() > 3 {
        return Err(anyhow!("Version string parse error: {}", my_version));
    }
    for (my, min) in my_version_parts.iter().zip(min_version_parts.iter()) {
        let my_num: u32 = my.parse()?;
        let min_num: u32 = min.parse()?;

        if my_num != min_num {
            return Ok(my_num > min_num);
        }
    }

    Ok(my_version_parts.len() >= min_version_parts.len())
}
