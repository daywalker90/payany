use anyhow::{Error, anyhow};
use cln_plugin::Plugin;
use serde_json::{Map, json};

use crate::{PluginState, fetch::resolve_invstring};

const PAYANYARGS: [&str; 3] = ["invstring", "amount_msat", "message"];

pub async fn payany(
    plugin: Plugin<PluginState>,
    args: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let mut params = Map::new();
    if let Some(args_obj) = args.as_object() {
        params.clone_from(args_obj);
    } else if let Some(args_arr) = args.as_array() {
        for (i, arg) in args_arr.iter().enumerate() {
            params.insert(PAYANYARGS[i].to_owned(), arg.clone());
        }
    }
    match resolve_invstring(plugin, &mut params).await {
        Ok(o) => o,
        Err(e) => {
            params.remove("message");
            return Err(anyhow!(e.to_string()));
        }
    }
    Ok(json!({"invoice":format!("{}", params.get("invstring").unwrap().as_str().unwrap())}))
}
