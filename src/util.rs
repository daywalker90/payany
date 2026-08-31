use std::path::Path;

use anyhow::anyhow;
use cln_plugin::ConfiguredPlugin;
use cln_rpc::{ClnRpc, model::requests::SetconfigRequest};
use serde_json::json;

use crate::PluginState;

pub async fn check_handle_option(
    plugin: &ConfiguredPlugin<PluginState, tokio::io::Stdin, tokio::io::Stdout>,
) -> Result<(), anyhow::Error> {
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
        tokio::spawn(async move {
            if let Err(e) = rpc
                .call_typed(&SetconfigRequest {
                    transient: Some(true),
                    val: Some("false".to_owned()),
                    config: "xpay-handle-pay".to_owned(),
                })
                .await
            {
                log::warn!("{e}");
            }
        });
        log::info!("Found activated `xpay-handle-pay`, `payany` deactivated it!");
    }
    Ok(())
}
