// Copyright 2026 Ernesto Ruge
// Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::X2JError;
use crate::schema::model::*;

/// Convert an XML file to JSON using the provided schema model.
/// Reads XML via streaming, writes JSON to the output path.
pub fn convert_file(
    xml_path: &Path,
    schema: &SchemaModel,
    output_path: &Path,
) -> Result<(), X2JError> {
    let file = File::open(xml_path)
        .map_err(|e| X2JError::Io(format!("Failed to open {}: {}", xml_path.display(), e)))?;
    let reader = BufReader::with_capacity(8 * 1024 * 1024, file); // 8 MB buffer
    let out_file = File::create(output_path)
        .map_err(|e| X2JError::Io(format!("Failed to create {}: {}", output_path.display(), e)))?;
    let writer = BufWriter::with_capacity(8 * 1024 * 1024, out_file);

    convert_reader(reader, schema, writer)
}

/// Convert XML to a JSON string in memory.
pub fn convert_to_string(xml: &str, schema: &SchemaModel) -> Result<String, X2JError> {
    let reader = BufReader::new(xml.as_bytes());
    let mut output = Vec::new();
    convert_reader(reader, schema, &mut output)?;
    String::from_utf8(output).map_err(|e| X2JError::Conversion(format!("UTF-8 error: {}", e)))
}

/// Core conversion: streaming XML reader → JSON writer.
///
/// Strategy: We build an in-memory tree of JsonNode values for each element,
/// then serialize. For very large files, this is still efficient because we
/// process element by element and the schema tells us the structure.
///
/// A fully streaming approach (writing JSON as we read XML) is possible but
/// significantly more complex due to arrays needing to be opened before knowing
/// all siblings. This tree approach works well for files up to several hundred MB.
fn convert_reader<R: BufRead, W: Write>(
    reader: R,
    schema: &SchemaModel,
    mut writer: W,
) -> Result<(), X2JError> {
    let mut xml_reader = Reader::from_reader(reader);
    xml_reader.trim_text(true);
    let mut buf = Vec::with_capacity(4096);

    // Stack of (element_def, json_node) pairs being built
    let mut stack: Vec<BuildFrame> = Vec::new();
    let mut root_node: Option<JsonNode> = None;
    let mut root_name: Option<String> = None;

    loop {
        match xml_reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_prefix(&tag_name);

                // Find the element definition
                let elem_def = find_elem_def(local, &stack, schema);
                let elem_def = override_with_xsi_type(e, elem_def, schema);

                // Collect attributes
                let mut attrs: HashMap<String, serde_json::Value> = HashMap::new();
                if let Some(def) = &elem_def {
                    if let TypeDef::Complex(complex) = &def.type_def {
                        for attr in e.attributes().flatten() {
                            let key =
                                String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            let attr_def = complex.attributes.iter().find(|a| a.name == key);
                            let json_val = if let Some(ad) = attr_def {
                                coerce_value(&val, ad.json_type)
                            } else {
                                serde_json::Value::String(val)
                            };
                            attrs.insert(format!("@{}", key), json_val);
                        }
                    }
                }

                let node = JsonNode {
                    children: HashMap::new(),
                    text: None,
                    attributes: attrs,
                };

                stack.push(BuildFrame {
                    name: local.to_string(),
                    elem_def,
                    node,
                });
            }
            Ok(Event::Text(ref e)) => {
                if let Some(frame) = stack.last_mut() {
                    let text = e.unescape().unwrap_or_default().to_string();
                    if !text.is_empty() {
                        frame.node.text = Some(text);
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                if let Some(frame) = stack.last_mut() {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    if !text.is_empty() {
                        frame.node.text = Some(text);
                    }
                }
            }
            Ok(Event::End(_)) => {
                if let Some(frame) = stack.pop() {
                    let json_value = frame_to_json(&frame);
                    if let Some(parent) = stack.last_mut() {
                        let is_array = frame
                            .elem_def
                            .as_ref()
                            .map(|d| d.occurrence.is_array())
                            .unwrap_or(false);

                        parent
                            .node
                            .children
                            .entry(frame.name.clone())
                            .or_insert_with(|| ChildSlot {
                                values: Vec::new(),
                                is_array,
                            })
                            .values
                            .push(json_value);
                    } else {
                        // Root element
                        root_name = Some(frame.name.clone());
                        root_node = Some(frame.node);
                        // Don't break yet — serialize after Eof
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = strip_prefix(&tag_name);
                let elem_def = find_elem_def(local, &stack, schema);
                let elem_def = override_with_xsi_type(e, elem_def, schema);

                let mut attrs: HashMap<String, serde_json::Value> = HashMap::new();
                if let Some(def) = &elem_def {
                    if let TypeDef::Complex(complex) = &def.type_def {
                        for attr in e.attributes().flatten() {
                            let key =
                                String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            let attr_def = complex.attributes.iter().find(|a| a.name == key);
                            let json_val = if let Some(ad) = attr_def {
                                coerce_value(&val, ad.json_type)
                            } else {
                                serde_json::Value::String(val)
                            };
                            attrs.insert(format!("@{}", key), json_val);
                        }
                    }
                }

                // Empty element with possible attributes
                let is_array = elem_def
                    .as_ref()
                    .map(|d| d.occurrence.is_array())
                    .unwrap_or(false);

                let json_value = if attrs.is_empty() {
                    serde_json::Value::Null
                } else {
                    let mut map = serde_json::Map::new();
                    for (k, v) in attrs {
                        map.insert(k, v);
                    }
                    serde_json::Value::Object(map)
                };

                if let Some(parent) = stack.last_mut() {
                    parent
                        .node
                        .children
                        .entry(local.to_string())
                        .or_insert_with(|| ChildSlot {
                            values: Vec::new(),
                            is_array,
                        })
                        .values
                        .push(json_value);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(X2JError::Conversion(format!("XML read error: {}", e))),
        }
        buf.clear();
    }

    // Serialize root
    if let Some(root) = root_node {
        let root_json = node_to_json(&root);
        let mut root_map = serde_json::Map::new();
        root_map.insert(
            root_name.unwrap_or_else(|| "root".to_string()),
            root_json,
        );
        let output = serde_json::Value::Object(root_map);
        serde_json::to_writer_pretty(&mut writer, &output)
            .map_err(|e| X2JError::Conversion(format!("JSON write error: {}", e)))?;
    }

    writer
        .flush()
        .map_err(|e| X2JError::Io(format!("Flush error: {}", e)))?;
    Ok(())
}

// --- Internal types ---

struct BuildFrame {
    name: String,
    elem_def: Option<ElementDef>,
    node: JsonNode,
}

struct JsonNode {
    children: HashMap<String, ChildSlot>,
    text: Option<String>,
    attributes: HashMap<String, serde_json::Value>,
}

struct ChildSlot {
    values: Vec<serde_json::Value>,
    is_array: bool,
}

// --- Helpers ---

fn strip_prefix(tag: &str) -> &str {
    tag.split_once(':').map(|(_, l)| l).unwrap_or(tag)
}

/// Override element type using xsi:type attribute if present
fn override_with_xsi_type(
    e: &quick_xml::events::BytesStart,
    elem_def: Option<ElementDef>,
    schema: &SchemaModel,
) -> Option<ElementDef> {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        if key == "xsi:type" {
            let val = String::from_utf8_lossy(&attr.value).to_string();
            let type_local = strip_prefix(&val);
            if let Some(type_def) = schema.named_types.get(type_local) {
                let mut def = elem_def.unwrap_or_else(|| ElementDef {
                    name: QName::new(""),
                    type_def: TypeDef::Simple(JsonType::String),
                    occurrence: Occurrence::default(),
                });
                def.type_def = type_def.clone();
                return Some(def);
            }
        }
    }
    elem_def
}

/// Find the ElementDef for a child element, looking at the parent's type definition
fn find_elem_def(
    local_name: &str,
    stack: &[BuildFrame],
    schema: &SchemaModel,
) -> Option<ElementDef> {
    if stack.is_empty() {
        // Root element
        return schema.get_element(local_name).cloned();
    }

    // Look at the parent's type definition
    if let Some(parent) = stack.last() {
        if let Some(ref def) = parent.elem_def {
            if let TypeDef::Complex(ref complex) = def.type_def {
                return SchemaModel::find_child_def(complex, local_name).cloned();
            }
        }
    }

    // Fallback: check top-level elements
    schema.get_element(local_name).cloned()
}

/// Convert a text value to a serde_json::Value based on the target JSON type
fn coerce_value(text: &str, json_type: JsonType) -> serde_json::Value {
    match json_type {
        JsonType::String => serde_json::Value::String(text.to_string()),
        JsonType::Integer => text
            .parse::<i64>()
            .map(|n| serde_json::Value::Number(n.into()))
            .unwrap_or_else(|_| serde_json::Value::String(text.to_string())),
        JsonType::Float => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(text.to_string())),
        JsonType::Boolean => match text {
            "true" | "1" => serde_json::Value::Bool(true),
            "false" | "0" => serde_json::Value::Bool(false),
            _ => serde_json::Value::String(text.to_string()),
        },
        JsonType::Object => serde_json::Value::String(text.to_string()),
    }
}

/// Convert a BuildFrame into a serde_json::Value
fn frame_to_json(frame: &BuildFrame) -> serde_json::Value {
    let type_def = frame.elem_def.as_ref().map(|d| &d.type_def);

    // Simple type with text content → coerce to the right JSON type
    if frame.node.children.is_empty() && frame.node.attributes.is_empty() {
        if let Some(text) = &frame.node.text {
            let json_type = type_def
                .map(|td| td.json_type())
                .unwrap_or(JsonType::String);
            return coerce_value(text, json_type);
        }
        // No text, no children → null
        return serde_json::Value::Null;
    }

    node_to_json(&frame.node)
}

/// Convert a JsonNode to a serde_json::Value
fn node_to_json(node: &JsonNode) -> serde_json::Value {
    let mut map = serde_json::Map::new();

    // Add attributes
    for (key, val) in &node.attributes {
        map.insert(key.clone(), val.clone());
    }

    // Add text content if mixed with children
    if let Some(text) = &node.text {
        if !node.children.is_empty() || !node.attributes.is_empty() {
            map.insert("_text".to_string(), serde_json::Value::String(text.clone()));
        }
    }

    // Add children
    for (name, slot) in &node.children {
        let value = if slot.is_array {
            serde_json::Value::Array(slot.values.clone())
        } else if slot.values.len() == 1 {
            slot.values[0].clone()
        } else {
            // Multiple values but schema says not array — shouldn't happen, but be safe
            serde_json::Value::Array(slot.values.clone())
        };
        map.insert(name.clone(), value);
    }

    serde_json::Value::Object(map)
}
