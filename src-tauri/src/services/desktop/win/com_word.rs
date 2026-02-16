use serde_json::{json, Value};
use windows::core::VARIANT;

use super::com_automation::{
    call_method, get_property, variant_from_dispatch, variant_to_dispatch, variant_to_i32,
    variant_to_string, with_active_application,
};
use super::{read_bool, read_i64, read_string};

const WORD_PROG_ID: &str = "Word.Application";

pub(super) async fn execute(action: &str, params: &Value) -> Result<Option<Value>, String> {
    let result = match action {
        "word_get_doc_info" => with_active_application(WORD_PROG_ID, |app| {
            let doc = variant_to_dispatch(&get_property(app, "ActiveDocument")?)?;
            let name = variant_to_string(&get_property(&doc, "Name")?)?;
            let full_name = variant_to_string(&get_property(&doc, "FullName")?)?;

            let words = variant_to_dispatch(&get_property(&doc, "Words")?)?;
            let word_count = variant_to_i32(&get_property(&words, "Count")?).unwrap_or(0);

            let paragraphs = variant_to_dispatch(&get_property(&doc, "Paragraphs")?)?;
            let paragraph_count = variant_to_i32(&get_property(&paragraphs, "Count")?).unwrap_or(0);

            Ok(json!({
                "app": "word",
                "name": name,
                "full_name": full_name,
                "words": word_count,
                "paragraphs": paragraph_count
            }))
        })?,
        "word_insert_text" => {
            let text = read_string(params, "text")?;
            let at_end = read_bool(params, "at_end", true);
            with_active_application(WORD_PROG_ID, |app| {
                let selection = variant_to_dispatch(&get_property(app, "Selection")?)?;
                if at_end {
                    let _ = call_method(&selection, "EndKey", vec![VARIANT::from(6i32)])?;
                }
                let _ = call_method(&selection, "TypeText", vec![VARIANT::from(text.as_str())])?;
                Ok(json!({
                    "app": "word",
                    "action": "insert_text",
                    "chars": text.chars().count()
                }))
            })?
        }
        "word_insert_table" => {
            let rows = read_i64(params, "rows", 2).clamp(1, 200) as i32;
            let cols =
                read_i64(params, "columns", read_i64(params, "cols", 2)).clamp(1, 100) as i32;
            with_active_application(WORD_PROG_ID, |app| {
                let doc = variant_to_dispatch(&get_property(app, "ActiveDocument")?)?;
                let selection = variant_to_dispatch(&get_property(app, "Selection")?)?;
                let range = variant_to_dispatch(&get_property(&selection, "Range")?)?;
                let tables = variant_to_dispatch(&get_property(&doc, "Tables")?)?;
                let range_arg = variant_from_dispatch(&range)?;
                let _ = call_method(
                    &tables,
                    "Add",
                    vec![range_arg, VARIANT::from(rows), VARIANT::from(cols)],
                )?;
                Ok(json!({
                    "app": "word",
                    "action": "insert_table",
                    "rows": rows,
                    "columns": cols
                }))
            })?
        }
        "word_save_as" => {
            let path = read_string(params, "path")?;
            with_active_application(WORD_PROG_ID, |app| {
                let doc = variant_to_dispatch(&get_property(app, "ActiveDocument")?)?;
                let _ = call_method(&doc, "SaveAs", vec![VARIANT::from(path.as_str())])?;
                Ok(json!({
                    "app": "word",
                    "action": "save_as",
                    "path": path
                }))
            })?
        }
        _ => return Ok(None),
    };

    Ok(Some(result))
}
