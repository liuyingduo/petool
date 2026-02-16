use serde_json::{json, Value};
use windows::core::VARIANT;

use super::com_automation::{
    call_method, get_property, set_property, variant_to_dispatch, variant_to_i32,
    variant_to_string, with_active_application,
};
use super::{read_i64, read_optional_string, read_string};

const PPT_PROG_ID: &str = "PowerPoint.Application";

pub(super) async fn execute(action: &str, params: &Value) -> Result<Option<Value>, String> {
    let result = match action {
        "ppt_get_presentation_info" => with_active_application(PPT_PROG_ID, |app| {
            let pres = variant_to_dispatch(&get_property(app, "ActivePresentation")?)?;
            let name = variant_to_string(&get_property(&pres, "Name")?)?;
            let full_name = variant_to_string(&get_property(&pres, "FullName")?)?;
            let slides = variant_to_dispatch(&get_property(&pres, "Slides")?)?;
            let slide_count = variant_to_i32(&get_property(&slides, "Count")?).unwrap_or(0);
            Ok(json!({
                "app": "ppt",
                "name": name,
                "full_name": full_name,
                "slides": slide_count
            }))
        })?,
        "ppt_add_slide" => {
            let index = read_i64(params, "index", 0).max(0) as i32;
            let layout = read_i64(params, "layout", 12).clamp(1, 36) as i32;
            with_active_application(PPT_PROG_ID, |app| {
                let pres = variant_to_dispatch(&get_property(app, "ActivePresentation")?)?;
                let slides = variant_to_dispatch(&get_property(&pres, "Slides")?)?;
                let count = variant_to_i32(&get_property(&slides, "Count")?).unwrap_or(0);
                let target = if index <= 0 {
                    count + 1
                } else {
                    index.clamp(1, count + 1)
                };
                let slide_var = call_method(
                    &slides,
                    "Add",
                    vec![VARIANT::from(target), VARIANT::from(layout)],
                )?;
                let slide = variant_to_dispatch(&slide_var)?;
                let slide_index =
                    variant_to_i32(&get_property(&slide, "SlideIndex")?).unwrap_or(target);
                Ok(json!({
                    "app": "ppt",
                    "action": "add_slide",
                    "slide_index": slide_index,
                    "layout": layout
                }))
            })?
        }
        "ppt_set_text" => {
            let slide_index = read_i64(params, "slide_index", 1).clamp(1, 5000) as i32;
            let text = read_string(params, "text")?;
            let left = read_i64(params, "left", 60).clamp(0, 3000) as i32;
            let top = read_i64(params, "top", 60).clamp(0, 3000) as i32;
            let width = read_i64(params, "width", 700).clamp(10, 5000) as i32;
            let height = read_i64(params, "height", 120).clamp(10, 5000) as i32;
            let shape_name = read_optional_string(params, "shape_name").unwrap_or_default();

            with_active_application(PPT_PROG_ID, |app| {
                let pres = variant_to_dispatch(&get_property(app, "ActivePresentation")?)?;
                let slides = variant_to_dispatch(&get_property(&pres, "Slides")?)?;
                let slide_var = call_method(&slides, "Item", vec![VARIANT::from(slide_index)])?;
                let slide = variant_to_dispatch(&slide_var)?;
                let shapes = variant_to_dispatch(&get_property(&slide, "Shapes")?)?;

                let shape = if shape_name.trim().is_empty() {
                    let shape_var = call_method(
                        &shapes,
                        "AddTextbox",
                        vec![
                            VARIANT::from(1i32),
                            VARIANT::from(left),
                            VARIANT::from(top),
                            VARIANT::from(width),
                            VARIANT::from(height),
                        ],
                    )?;
                    variant_to_dispatch(&shape_var)?
                } else {
                    let shape_var =
                        call_method(&shapes, "Item", vec![VARIANT::from(shape_name.as_str())])?;
                    variant_to_dispatch(&shape_var)?
                };

                let text_frame = variant_to_dispatch(&get_property(&shape, "TextFrame")?)?;
                let text_range = variant_to_dispatch(&get_property(&text_frame, "TextRange")?)?;
                set_property(&text_range, "Text", VARIANT::from(text.as_str()))?;
                let final_shape_name = variant_to_string(&get_property(&shape, "Name")?)
                    .unwrap_or_else(|_| "".to_string());

                Ok(json!({
                    "app": "ppt",
                    "action": "set_text",
                    "slide_index": slide_index,
                    "shape_name": final_shape_name
                }))
            })?
        }
        "ppt_save_as" => {
            let path = read_string(params, "path")?;
            with_active_application(PPT_PROG_ID, |app| {
                let pres = variant_to_dispatch(&get_property(app, "ActivePresentation")?)?;
                let _ = call_method(&pres, "SaveAs", vec![VARIANT::from(path.as_str())])?;
                Ok(json!({
                    "app": "ppt",
                    "action": "save_as",
                    "path": path
                }))
            })?
        }
        _ => return Ok(None),
    };

    Ok(Some(result))
}
