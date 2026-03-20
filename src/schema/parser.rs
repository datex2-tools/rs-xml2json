// Copyright 2026 Ernesto Ruge
// Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::model::*;
use super::type_map::xsd_builtin_to_json;
use crate::error::X2JError;

const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema";

/// Parse an XSD file and all its imports/includes into a SchemaModel.
pub fn parse_xsd(xsd_path: &Path) -> Result<SchemaModel, X2JError> {
    let mut ctx = ParseContext::new(xsd_path)?;
    ctx.parse_schema_file(xsd_path)?;
    ctx.resolve_all()?;
    Ok(ctx.into_model())
}

/// Parse an XSD from an in-memory string.
pub fn parse_xsd_from_str(xsd: &str) -> Result<SchemaModel, X2JError> {
    let dummy_path = Path::new("inline.xsd");
    let mut ctx = ParseContext::new(dummy_path)?;
    ctx.parse_schema_str(xsd, dummy_path)?;
    ctx.resolve_all()?;
    Ok(ctx.into_model())
}

/// Internal parsing context that accumulates definitions across multiple schema files.
struct ParseContext {
    base_dir: PathBuf,
    model: SchemaModel,
    /// Raw unresolved element refs: element_name -> ref target name
    element_refs: HashMap<String, String>,
    /// Raw unresolved type refs: element_name -> type name
    type_refs: HashMap<String, String>,
    /// Complex type extensions: type_name -> base_type_name
    extensions: HashMap<String, String>,
    /// SimpleType non-XSD base refs: type_name -> base_type_name
    simple_type_refs: HashMap<String, String>,
    /// Already-parsed schema file paths (to avoid circular includes)
    parsed_files: Vec<PathBuf>,
    /// Namespace prefix map from the most recently parsed schema
    ns_prefixes: HashMap<String, String>,
    /// Target namespace of the primary schema
    target_namespace: Option<String>,
}

impl ParseContext {
    fn new(xsd_path: &Path) -> Result<Self, X2JError> {
        let base_dir = xsd_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        Ok(Self {
            base_dir,
            model: SchemaModel::new(),
            element_refs: HashMap::new(),
            type_refs: HashMap::new(),
            extensions: HashMap::new(),
            simple_type_refs: HashMap::new(),
            parsed_files: Vec::new(),
            ns_prefixes: HashMap::new(),
            target_namespace: None,
        })
    }

    fn parse_schema_file(&mut self, path: &Path) -> Result<(), X2JError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if self.parsed_files.contains(&canonical) {
            return Ok(()); // already parsed
        }
        self.parsed_files.push(canonical);

        let xml_content = fs::read_to_string(path)
            .map_err(|e| X2JError::Io(format!("Failed to read {}: {}", path.display(), e)))?;

        self.parse_schema_str(&xml_content, path)
    }

    fn parse_schema_str(&mut self, xml: &str, source_path: &Path) -> Result<(), X2JError> {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();
        let mut stack: Vec<XsdStackFrame> = Vec::new();

        // First pass: collect namespace prefixes from the root element
        self.collect_ns_prefixes(xml);

        loop {
            let event = reader.read_event_into(&mut buf);
            match event {
                Ok(Event::Start(ref e)) => {
                    self.handle_start_element(e, &mut stack, source_path)?;
                }
                Ok(Event::Empty(ref e)) => {
                    // Empty elements: handle as start + immediate end
                    self.handle_start_element(e, &mut stack, source_path)?;
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = local_name(&tag_name);
                    self.handle_end_element(&local, &mut stack)?;
                }
                Ok(Event::End(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let local = local_name(&tag_name);
                    self.handle_end_element(&local, &mut stack)?;
                }
                Ok(Event::Eof) => break,
                Ok(_) => {} // text, comments, etc.
                Err(e) => return Err(X2JError::XsdParse(format!("XML parse error: {}", e))),
            }
            buf.clear();
        }
        Ok(())
    }

    fn collect_ns_prefixes(&mut self, xml: &str) {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    for attr in e.attributes().with_checks(false) {
                        if let Ok(attr) = attr {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            if key == "targetNamespace" && self.target_namespace.is_none() {
                                self.target_namespace = Some(val.clone());
                            }
                            if let Some(prefix) = key.strip_prefix("xmlns:") {
                                self.ns_prefixes.insert(prefix.to_string(), val);
                            }
                        }
                    }
                    break; // only need root element
                }
                Ok(Event::Eof) => break,
                _ => continue, // skip PI, text, comments, etc.
            }
        }
    }

    /// Resolve a possibly prefixed type name (e.g. "xs:string") to (namespace, local_name)
    fn resolve_prefixed_name(&self, name: &str) -> (Option<String>, String) {
        if let Some((prefix, local)) = name.split_once(':') {
            let ns = self.ns_prefixes.get(prefix).cloned();
            (ns, local.to_string())
        } else {
            (self.target_namespace.clone(), name.to_string())
        }
    }

    fn handle_start_element(
        &mut self,
        e: &quick_xml::events::BytesStart,
        stack: &mut Vec<XsdStackFrame>,
        _source_path: &Path,
    ) -> Result<(), X2JError> {
        let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
        let local = local_name(&tag_name);
        let attrs = collect_attrs(e);

        match local.as_str() {
            "schema" => {
                stack.push(XsdStackFrame::Schema);
            }
            "import" | "include" => {
                if let Some(schema_loc) = attrs.get("schemaLocation") {
                    let import_path = self.base_dir.join(schema_loc);
                    if import_path.exists() {
                        self.parse_schema_file(&import_path)?;
                    }
                }
            }
            "element" => {
                self.handle_element(&attrs, stack)?;
            }
            "complexType" => {
                let name = attrs.get("name").cloned();
                let mixed = attrs.get("mixed").map(|v| v == "true").unwrap_or(false);
                stack.push(XsdStackFrame::ComplexType {
                    name,
                    children: Vec::new(),
                    attributes: Vec::new(),
                    mixed,
                    simple_content_type: None,
                });
            }
            "simpleType" => {
                let name = attrs.get("name").cloned();
                stack.push(XsdStackFrame::SimpleType {
                    name,
                    base_type: None,
                });
            }
            "sequence" | "all" | "choice" => {
                stack.push(XsdStackFrame::Compositor);
            }
            "extension" | "restriction" => {
                if let Some(base) = attrs.get("base") {
                    let (ns, base_local) = self.resolve_prefixed_name(base);
                    let is_xsd = ns.as_deref() == Some(XSD_NS);

                    if let Some(frame) = stack.last_mut() {
                        match frame {
                            XsdStackFrame::SimpleType { name, base_type, .. } => {
                                if is_xsd {
                                    *base_type = xsd_builtin_to_json(&base_local);
                                } else if let Some(type_name) = name.as_ref() {
                                    self.simple_type_refs
                                        .insert(type_name.clone(), base_local.clone());
                                }
                            }
                            XsdStackFrame::ComplexType {
                                name,
                                simple_content_type,
                                ..
                            } => {
                                if is_xsd {
                                    *simple_content_type = xsd_builtin_to_json(&base_local);
                                } else if let Some(type_name) = name.as_ref() {
                                    self.extensions
                                        .insert(type_name.clone(), base_local.clone());
                                }
                            }
                            _ => {}
                        }
                    }

                    stack.push(XsdStackFrame::Derivation {
                        _kind: if local == "extension" {
                            DerivationKind::Extension
                        } else {
                            DerivationKind::Restriction
                        },
                        _base_local: base_local,
                        _is_xsd_builtin: is_xsd,
                    });
                }
            }
            "attribute" => {
                let attr_name = attrs.get("name").cloned().unwrap_or_default();
                let type_name = attrs.get("type").cloned().unwrap_or_default();
                let required = attrs.get("use").map(|v| v == "required").unwrap_or(false);

                let (ns, type_local) = self.resolve_prefixed_name(&type_name);
                let json_type = if ns.as_deref() == Some(XSD_NS) {
                    xsd_builtin_to_json(&type_local).unwrap_or(JsonType::String)
                } else {
                    JsonType::String
                };

                let attr_def = AttributeDef {
                    name: attr_name,
                    json_type,
                    required,
                };

                // Add to the nearest ComplexType on the stack
                for frame in stack.iter_mut().rev() {
                    if let XsdStackFrame::ComplexType { attributes, .. } = frame {
                        attributes.push(attr_def);
                        break;
                    }
                }
            }
            "simpleContent" | "complexContent" => {
                // These are containers; the actual derivation is inside (extension/restriction)
                // We don't need a stack frame, but we need to mark the parent ComplexType
                // that it may have simple content. We'll handle this in extension/restriction.
            }
            _ => {
                // Other XSD elements (annotation, documentation, etc.) — ignore
            }
        }
        Ok(())
    }

    fn handle_element(
        &mut self,
        attrs: &HashMap<String, String>,
        stack: &mut Vec<XsdStackFrame>,
    ) -> Result<(), X2JError> {
        let name = attrs.get("name").cloned();
        let ref_attr = attrs.get("ref").cloned();
        let type_attr = attrs.get("type").cloned();
        let min_occurs = attrs
            .get("minOccurs")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1);
        let max_occurs = attrs
            .get("maxOccurs")
            .map(|v| {
                if v == "unbounded" {
                    MaxOccurs::Unbounded
                } else {
                    MaxOccurs::Bounded(v.parse::<u64>().unwrap_or(1))
                }
            })
            .unwrap_or(MaxOccurs::Bounded(1));

        let occurrence = Occurrence {
            min: min_occurs,
            max: max_occurs,
        };

        // If this is a ref, we'll resolve it later
        if let Some(ref_name) = ref_attr {
            let (_, ref_local) = self.resolve_prefixed_name(&ref_name);
            let elem_def = ElementDef {
                name: QName::new(&ref_local),
                type_def: TypeDef::Simple(JsonType::String), // placeholder
                occurrence,
            };

            // Add as child to parent complex type
            self.add_child_to_parent(elem_def, stack);
            // Track the ref for resolution
            if let Some(elem_name) = name.as_ref() {
                self.element_refs.insert(elem_name.clone(), ref_local);
            }
            return Ok(());
        }

        let elem_name = match name {
            Some(n) => n,
            None => return Ok(()), // anonymous, skip
        };

        // Determine type
        let has_type_attr = type_attr.is_some();
        let type_def = if let Some(type_name) = type_attr {
            let (ns, type_local) = self.resolve_prefixed_name(&type_name);
            if ns.as_deref() == Some(XSD_NS) {
                TypeDef::Simple(xsd_builtin_to_json(&type_local).unwrap_or(JsonType::String))
            } else {
                // Reference to a named type — store as placeholder, resolve later
                self.type_refs
                    .insert(elem_name.clone(), type_local.clone());
                TypeDef::Simple(JsonType::String) // placeholder
            }
        } else {
            // Type will be defined inline (child complexType/simpleType)
            TypeDef::Simple(JsonType::String) // placeholder until inline type is parsed
        };

        let elem_def = ElementDef {
            name: QName::new(&elem_name),
            type_def,
            occurrence,
        };

        // Determine where to put this element
        let is_top_level = stack
            .last()
            .map(|f| matches!(f, XsdStackFrame::Schema))
            .unwrap_or(false);

        if is_top_level {
            self.model.elements.insert(elem_name.clone(), elem_def);
        } else {
            self.add_child_to_parent(elem_def, stack);
        }

        // Push a frame so inline type definitions can attach to this element
        stack.push(XsdStackFrame::Element {
            name: elem_name,
            _has_inline_type: !has_type_attr,
        });

        Ok(())
    }

    fn add_child_to_parent(&self, elem_def: ElementDef, stack: &mut [XsdStackFrame]) {
        for frame in stack.iter_mut().rev() {
            if let XsdStackFrame::ComplexType { children, .. } = frame {
                children.push(elem_def);
                return;
            }
        }
    }

    fn handle_end_element(
        &mut self,
        local: &str,
        stack: &mut Vec<XsdStackFrame>,
    ) -> Result<(), X2JError> {
        match local {
            "complexType" => {
                if let Some(XsdStackFrame::ComplexType {
                    name,
                    children,
                    attributes,
                    mixed,
                    simple_content_type,
                }) = stack.pop()
                {
                    let complex_def = ComplexTypeDef {
                        children,
                        attributes,
                        mixed,
                        simple_content_type,
                    };
                    let type_def = TypeDef::Complex(complex_def);

                    if let Some(type_name) = name {
                        // Named type — register it
                        self.model.named_types.insert(type_name, type_def);
                    } else {
                        // Inline type — attach to parent element
                        self.attach_inline_type(type_def, stack);
                    }
                }
            }
            "simpleType" => {
                if let Some(XsdStackFrame::SimpleType { name, base_type }) = stack.pop() {
                    let json_type = base_type.unwrap_or(JsonType::String);
                    let type_def = TypeDef::Simple(json_type);

                    if let Some(type_name) = name {
                        self.model.named_types.insert(type_name, type_def);
                    } else {
                        self.attach_inline_type(type_def, stack);
                    }
                }
            }
            "element" => {
                // Pop element frame if it's on top
                if matches!(stack.last(), Some(XsdStackFrame::Element { .. })) {
                    stack.pop();
                }
            }
            "sequence" | "all" | "choice" => {
                if matches!(stack.last(), Some(XsdStackFrame::Compositor)) {
                    stack.pop();
                }
            }
            "extension" | "restriction" => {
                if matches!(stack.last(), Some(XsdStackFrame::Derivation { .. })) {
                    stack.pop();
                }
            }
            "schema" => {
                stack.pop();
            }
            _ => {}
        }
        Ok(())
    }

    /// Attach an inline type definition to the nearest parent element
    fn attach_inline_type(&mut self, type_def: TypeDef, stack: &mut [XsdStackFrame]) {
        // Look for a parent Element frame
        for frame in stack.iter().rev() {
            if let XsdStackFrame::Element { name, .. } = frame {
                // Update the element in the model or in a parent complex type
                let elem_name = name.clone();

                // Check top-level elements first
                if let Some(elem) = self.model.elements.get_mut(&elem_name) {
                    elem.type_def = type_def.clone();
                    return;
                }

                // Check parent complex type's children
                for frame in stack.iter_mut().rev() {
                    if let XsdStackFrame::ComplexType { children, .. } = frame {
                        for child in children.iter_mut() {
                            if child.name.local_name == elem_name {
                                child.type_def = type_def;
                                return;
                            }
                        }
                    }
                }
                return;
            }
        }
    }

    /// Second pass: resolve type references, element refs, and inheritance
    fn resolve_all(&mut self) -> Result<(), X2JError> {
        // 1. Resolve simpleType reference chains (e.g., Percentage -> Float -> xs:float)
        let simple_type_refs = self.simple_type_refs.clone();
        for (type_name, _) in &simple_type_refs {
            let mut current = type_name.clone();
            let mut resolved = None;
            for _ in 0..20 {
                if let Some(base_name) = simple_type_refs.get(&current) {
                    if let Some(TypeDef::Simple(jt)) = self.model.named_types.get(base_name) {
                        resolved = Some(*jt);
                        break;
                    }
                    current = base_name.clone();
                } else {
                    break;
                }
            }
            if let Some(json_type) = resolved {
                self.model
                    .named_types
                    .insert(type_name.clone(), TypeDef::Simple(json_type));
            }
        }

        // 2. Merge inherited children for complex type extensions
        let extensions = self.extensions.clone();
        let named_types_for_inheritance = self.model.named_types.clone();
        for (type_name, _) in &extensions {
            let mut inherited_children = Vec::new();
            let mut inherited_attributes = Vec::new();
            let mut current_base = extensions.get(type_name).cloned();
            let mut visited = vec![type_name.clone()];
            while let Some(base) = current_base {
                if visited.contains(&base) {
                    break;
                }
                visited.push(base.clone());
                if let Some(TypeDef::Complex(complex)) =
                    named_types_for_inheritance.get(&base)
                {
                    inherited_children.extend(complex.children.iter().cloned());
                    inherited_attributes.extend(complex.attributes.iter().cloned());
                }
                current_base = extensions.get(&base).cloned();
            }
            if !inherited_children.is_empty() || !inherited_attributes.is_empty() {
                if let Some(TypeDef::Complex(complex)) =
                    self.model.named_types.get_mut(type_name)
                {
                    let mut all_children = inherited_children;
                    all_children.append(&mut complex.children);
                    complex.children = all_children;
                    let mut all_attrs = inherited_attributes;
                    all_attrs.append(&mut complex.attributes);
                    complex.attributes = all_attrs;
                }
            }
        }

        // 3. Resolve type references for top-level elements
        let type_refs = self.type_refs.clone();
        for (elem_name, type_local) in &type_refs {
            if let Some(resolved_type) = self.model.named_types.get(type_local).cloned() {
                if let Some(elem) = self.model.elements.get_mut(elem_name) {
                    elem.type_def = resolved_type;
                }
            }
        }

        // 4. Resolve type references within named complex types (children referencing named types)
        let named_types_snapshot = self.model.named_types.clone();
        for (type_name, type_def) in self.model.named_types.iter_mut() {
            let mut resolving = HashSet::new();
            resolving.insert(type_name.clone());
            Self::resolve_children_types(type_def, &named_types_snapshot, &type_refs, &mut resolving);
        }
        for elem in self.model.elements.values_mut() {
            let mut resolving = HashSet::new();
            Self::resolve_children_types(&mut elem.type_def, &named_types_snapshot, &type_refs, &mut resolving);
        }

        self.model.target_namespace = self.target_namespace.clone();
        Ok(())
    }

    fn resolve_children_types(
        type_def: &mut TypeDef,
        named_types: &HashMap<String, TypeDef>,
        type_refs: &HashMap<String, String>,
        resolving: &mut HashSet<String>,
    ) {
        if let TypeDef::Complex(complex) = type_def {
            for child in &mut complex.children {
                if let Some(type_name) = type_refs.get(&child.name.local_name) {
                    if let Some(resolved) = named_types.get(type_name) {
                        child.type_def = resolved.clone();
                    }
                    // Only recurse if this type isn't already being expanded
                    // (prevents infinite recursion on circular type references)
                    if resolving.insert(type_name.clone()) {
                        Self::resolve_children_types(
                            &mut child.type_def,
                            named_types,
                            type_refs,
                            resolving,
                        );
                        resolving.remove(type_name);
                    }
                } else {
                    Self::resolve_children_types(
                        &mut child.type_def,
                        named_types,
                        type_refs,
                        resolving,
                    );
                }
            }
        }
    }

    fn into_model(self) -> SchemaModel {
        self.model
    }
}

// --- Stack frames for tracking parser state ---

#[derive(Debug)]
enum XsdStackFrame {
    Schema,
    ComplexType {
        name: Option<String>,
        children: Vec<ElementDef>,
        attributes: Vec<AttributeDef>,
        mixed: bool,
        simple_content_type: Option<JsonType>,
    },
    SimpleType {
        name: Option<String>,
        base_type: Option<JsonType>,
    },
    Element {
        name: String,
        _has_inline_type: bool,
    },
    Compositor, // sequence, all, choice
    Derivation {
        _kind: DerivationKind,
        _base_local: String,
        _is_xsd_builtin: bool,
    },
}

#[derive(Debug)]
enum DerivationKind {
    Extension,
    Restriction,
}

// --- Helpers ---

fn local_name(tag: &str) -> String {
    if let Some((_prefix, local)) = tag.split_once(':') {
        local.to_string()
    } else {
        tag.to_string()
    }
}

fn collect_attrs(e: &quick_xml::events::BytesStart) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();
        map.insert(key, val);
    }
    map
}
