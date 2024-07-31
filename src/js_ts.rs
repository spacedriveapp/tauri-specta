use std::borrow::Cow;

use heck::ToLowerCamelCase;
use indoc::formatdoc;
use specta::{
    datatype,
    datatype::{DataType, FunctionResultVariant},
    TypeMap,
};
use specta_typescript as ts;
use specta_typescript::ExportError;

use crate::{EventDataType, ExportLanguage, ItemType, StaticCollection};

pub const DO_NOT_EDIT: &str = "// This file was generated by [tauri-specta](https://github.com/oscartbeaumont/tauri-specta). Do not edit this file manually.";
const CRINGE_ESLINT_DISABLE: &str = "/* eslint-disable */
";

pub type ExportConfig = crate::ExportConfig<specta_typescript::Typescript>;

pub fn render_all_parts<T: ExportLanguage<Config = specta_typescript::Typescript>>(
    commands: &[datatype::Function],
    events: &[EventDataType],
    type_map: &TypeMap,
    statics: &StaticCollection,
    cfg: &ExportConfig,
    dependant_types: &str,
    globals: &str,
) -> Result<String, T::Error> {
    let commands = T::render_commands(commands, type_map, cfg)?;
    let events = T::render_events(events, type_map, cfg)?;

    let statics = statics
        .statics
        .iter()
        .map(|(name, value)| {
            let mut as_const = None;
            match &value {
                serde_json::Value::Null => {}
                serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_)
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => as_const = Some(" as const"),
            }

            format!(
                "export const {name} = {}{};",
                serde_json::to_string(&value)
                    .expect("failed to serialize from `serde_json::Value`"),
                as_const.unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(formatdoc! {
        r#"
            {DO_NOT_EDIT}

            /** user-defined commands **/

            {commands}

            /** user-defined events **/

			{events}

            /** user-defined statics **/

            {statics}

			/** user-defined types **/

			{dependant_types}

			/** tauri-specta globals **/

            {globals}
        "#
    })
}

pub fn arg_names(args: &[(Cow<'static, str>, DataType)]) -> Vec<String> {
    args.iter()
        .map(|(name, _)| name.to_lower_camel_case())
        .collect::<Vec<_>>()
}

pub fn arg_usages(args: &[String]) -> Option<String> {
    (!args.is_empty()).then(|| format!("{{ {} }}", args.join(", ")))
}

fn return_as_result_tuple(expr: &str, as_any: bool) -> String {
    let as_any = as_any.then_some(" as any").unwrap_or_default();

    formatdoc!(
        r#"
		try {{
		    return {{ status: "ok", data: {expr} }};
		}} catch (e) {{
		    if(e instanceof Error) throw e;
		    else return {{ status: "error", error: e {as_any} }};
		}}"#
    )
}

pub fn maybe_return_as_result_tuple(
    expr: &str,
    typ: Option<&FunctionResultVariant>,
    as_any: bool,
) -> String {
    match typ {
        Some(FunctionResultVariant::Result(_, _)) => return_as_result_tuple(expr, as_any),
        Some(FunctionResultVariant::Value(_)) => format!("return {expr};"),
        None => format!("{expr};"),
    }
}

pub fn function(
    docs: &str,
    name: &str,
    args: &[String],
    return_type: Option<&str>,
    body: &str,
) -> String {
    let args = args.join(", ");
    let return_type = return_type
        .map(|t| format!(": Promise<{}>", t))
        .unwrap_or_default();

    formatdoc! {
        r#"
		{docs}async {name}({args}) {return_type} {{
		{body}
		}}"#
    }
}

fn tauri_invoke(name: &str, arg_usages: Option<String>) -> String {
    let arg_usages = arg_usages.map(|u| format!(", {u}")).unwrap_or_default();

    format!(r#"await TAURI_INVOKE("{name}"{arg_usages})"#)
}

pub fn handle_result(
    function: &datatype::Function,
    type_map: &TypeMap,
    cfg: &ExportConfig,
) -> Result<String, ExportError> {
    Ok(match &function.result() {
        Some(FunctionResultVariant::Result(t, e)) => {
            format!(
                "Result<{}, {}>",
                ts::datatype(
                    &cfg.inner,
                    &FunctionResultVariant::Value(t.clone()),
                    type_map
                )?,
                ts::datatype(
                    &cfg.inner,
                    &FunctionResultVariant::Value(e.clone()),
                    type_map
                )?
            )
        }
        Some(FunctionResultVariant::Value(t)) => ts::datatype(
            &cfg.inner,
            &FunctionResultVariant::Value(t.clone()),
            type_map,
        )?,
        None => "void".to_string(),
    })
}

pub fn command_body(cfg: &ExportConfig, function: &datatype::Function, as_any: bool) -> String {
    let name = cfg
        .plugin_name
        .map(|n| n.apply_as_prefix(&function.name(), ItemType::Command))
        .unwrap_or_else(|| function.name().to_string());

    maybe_return_as_result_tuple(
        &tauri_invoke(
            &name,
            arg_usages(&arg_names(
                // TODO: Don't collect
                &function.args().cloned().collect::<Vec<_>>(),
            )),
        ),
        function.result(),
        as_any,
    )
}

pub fn events_map(events: &[EventDataType], cfg: &ExportConfig) -> String {
    events
        .iter()
        .map(|event| {
            let name_str = cfg
                .plugin_name
                .map(|n| n.apply_as_prefix(event.name, ItemType::Event))
                .unwrap_or_else(|| event.name.to_string());
            let name_camel = event.name.to_lower_camel_case();

            format!(r#"{name_camel}: "{name_str}""#)
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

pub fn events_types(
    events: &[EventDataType],
    cfg: &ExportConfig,
    type_map: &TypeMap,
) -> Result<Vec<String>, ExportError> {
    events
        .iter()
        .map(|event| {
            let name_camel = event.name.to_lower_camel_case();

            let typ = ts::datatype(
                &cfg.inner,
                &FunctionResultVariant::Value(event.typ.clone()),
                type_map,
            )?;

            Ok(format!(r#"{name_camel}: {typ}"#))
        })
        .collect()
}

pub fn events_data(
    events: &[EventDataType],
    cfg: &ExportConfig,
    type_map: &TypeMap,
) -> Result<(Vec<String>, String), ExportError> {
    Ok((
        events_types(events, cfg, type_map)?,
        events_map(events, cfg),
    ))
}

impl From<specta_typescript::Typescript> for ExportConfig {
    fn from(config: specta_typescript::Typescript) -> Self {
        Self {
            header: CRINGE_ESLINT_DISABLE.into(),
            ..Self::new(config)
        }
    }
}
