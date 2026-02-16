use serde_json::{json, Value};
use windows::core::VARIANT;

use super::com_automation::{
    call_method, get_property, set_property, variant_to_dispatch, variant_to_i32,
    variant_to_string, with_active_application,
};
use super::{read_i64, read_optional_string, read_string};

const EXCEL_PROG_ID: &str = "Excel.Application";

fn resolve_sheet(
    app: &windows::Win32::System::Com::IDispatch,
    workbook: &windows::Win32::System::Com::IDispatch,
    sheet_name: &str,
) -> Result<windows::Win32::System::Com::IDispatch, String> {
    if sheet_name.trim().is_empty() {
        return variant_to_dispatch(&get_property(app, "ActiveSheet")?);
    }
    let sheets = variant_to_dispatch(&get_property(workbook, "Worksheets")?)?;
    let item = call_method(&sheets, "Item", vec![VARIANT::from(sheet_name)])?;
    variant_to_dispatch(&item)
}

pub(super) async fn execute(action: &str, params: &Value) -> Result<Option<Value>, String> {
    let result = match action {
        "excel_get_workbook_info" => with_active_application(EXCEL_PROG_ID, |app| {
            let workbook = variant_to_dispatch(&get_property(app, "ActiveWorkbook")?)?;
            let name = variant_to_string(&get_property(&workbook, "Name")?)?;
            let full_name = variant_to_string(&get_property(&workbook, "FullName")?)?;
            let sheets = variant_to_dispatch(&get_property(&workbook, "Worksheets")?)?;
            let sheet_count = variant_to_i32(&get_property(&sheets, "Count")?).unwrap_or(0);
            let active_sheet = variant_to_dispatch(&get_property(app, "ActiveSheet")?)?;
            let active_sheet_name = variant_to_string(&get_property(&active_sheet, "Name")?)
                .unwrap_or_else(|_| "".to_string());

            Ok(json!({
                "app": "excel",
                "name": name,
                "full_name": full_name,
                "sheets": sheet_count,
                "active_sheet": active_sheet_name
            }))
        })?,
        "excel_set_cell" => {
            let sheet = read_optional_string(params, "sheet").unwrap_or_default();
            let row = read_i64(params, "row", 1).clamp(1, 1_000_000) as i32;
            let col =
                read_i64(params, "column", read_i64(params, "col", 1)).clamp(1, 16_000) as i32;
            let value = read_string(params, "value")?;

            with_active_application(EXCEL_PROG_ID, |app| {
                let workbook = variant_to_dispatch(&get_property(app, "ActiveWorkbook")?)?;
                let sheet_obj = resolve_sheet(app, &workbook, &sheet)?;
                let cells = variant_to_dispatch(&get_property(&sheet_obj, "Cells")?)?;
                let cell_var =
                    call_method(&cells, "Item", vec![VARIANT::from(row), VARIANT::from(col)])?;
                let cell = variant_to_dispatch(&cell_var)?;
                set_property(&cell, "Value2", VARIANT::from(value.as_str()))?;
                let sheet_name = variant_to_string(&get_property(&sheet_obj, "Name")?)
                    .unwrap_or_else(|_| "".to_string());
                Ok(json!({
                    "app": "excel",
                    "action": "set_cell",
                    "sheet": sheet_name,
                    "row": row,
                    "column": col
                }))
            })?
        }
        "excel_set_range" => {
            let sheet = read_optional_string(params, "sheet").unwrap_or_default();
            let address = read_string(params, "address")?;
            let value = read_string(params, "value")?;

            with_active_application(EXCEL_PROG_ID, |app| {
                let workbook = variant_to_dispatch(&get_property(app, "ActiveWorkbook")?)?;
                let sheet_obj = resolve_sheet(app, &workbook, &sheet)?;
                let range_var =
                    call_method(&sheet_obj, "Range", vec![VARIANT::from(address.as_str())])?;
                let range = variant_to_dispatch(&range_var)?;
                set_property(&range, "Value2", VARIANT::from(value.as_str()))?;
                let sheet_name = variant_to_string(&get_property(&sheet_obj, "Name")?)
                    .unwrap_or_else(|_| "".to_string());
                Ok(json!({
                    "app": "excel",
                    "action": "set_range",
                    "sheet": sheet_name,
                    "address": address
                }))
            })?
        }
        "excel_save_as" => {
            let path = read_string(params, "path")?;
            with_active_application(EXCEL_PROG_ID, |app| {
                let workbook = variant_to_dispatch(&get_property(app, "ActiveWorkbook")?)?;
                let _ = call_method(&workbook, "SaveAs", vec![VARIANT::from(path.as_str())])?;
                Ok(json!({
                    "app": "excel",
                    "action": "save_as",
                    "path": path
                }))
            })?
        }
        _ => return Ok(None),
    };

    Ok(Some(result))
}
