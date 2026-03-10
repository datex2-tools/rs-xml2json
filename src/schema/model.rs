// Copyright 2026 Ernesto Ruge
// Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

use std::collections::HashMap;

/// Qualified name: (namespace_uri, local_name)
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct QName {
    pub namespace: Option<String>,
    pub local_name: String,
}

impl QName {
    pub fn new(local_name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            local_name: local_name.into(),
        }
    }

    pub fn with_ns(namespace: impl Into<String>, local_name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            local_name: local_name.into(),
        }
    }
}

/// The JSON type an XSD type maps to
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JsonType {
    String,
    Integer,
    Float,
    Boolean,
    Object,
}

/// How many times an element can occur
#[derive(Debug, Clone)]
pub struct Occurrence {
    pub min: u64,
    pub max: MaxOccurs,
}

impl Default for Occurrence {
    fn default() -> Self {
        Self {
            min: 1,
            max: MaxOccurs::Bounded(1),
        }
    }
}

impl Occurrence {
    pub fn is_array(&self) -> bool {
        matches!(self.max, MaxOccurs::Unbounded) || matches!(self.max, MaxOccurs::Bounded(n) if n > 1)
    }

    pub fn is_optional(&self) -> bool {
        self.min == 0
    }
}

#[derive(Debug, Clone)]
pub enum MaxOccurs {
    Bounded(u64),
    Unbounded,
}

/// Definition of an XML element as understood from the schema
#[derive(Debug, Clone)]
pub struct ElementDef {
    pub name: QName,
    pub type_def: TypeDef,
    pub occurrence: Occurrence,
}

/// An XML attribute definition
#[derive(Debug, Clone)]
pub struct AttributeDef {
    pub name: String,
    pub json_type: JsonType,
    pub required: bool,
}

/// Type definition — either simple (maps to a JSON primitive) or complex (maps to a JSON object)
#[derive(Debug, Clone)]
pub enum TypeDef {
    Simple(JsonType),
    Complex(ComplexTypeDef),
}

impl TypeDef {
    pub fn json_type(&self) -> JsonType {
        match self {
            TypeDef::Simple(jt) => *jt,
            TypeDef::Complex(_) => JsonType::Object,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComplexTypeDef {
    pub children: Vec<ElementDef>,
    pub attributes: Vec<AttributeDef>,
    pub mixed: bool,
    /// If this complex type has simpleContent, store the base JSON type
    pub simple_content_type: Option<JsonType>,
}

/// The fully resolved schema model
#[derive(Debug)]
pub struct SchemaModel {
    /// Top-level element definitions (by local name)
    pub elements: HashMap<String, ElementDef>,
    /// Named type definitions (by local name) for lookup during resolution
    pub named_types: HashMap<String, TypeDef>,
    /// Target namespace of the primary schema
    pub target_namespace: Option<String>,
}

impl SchemaModel {
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            named_types: HashMap::new(),
            target_namespace: None,
        }
    }

    /// Look up a top-level element by local name
    pub fn get_element(&self, local_name: &str) -> Option<&ElementDef> {
        self.elements.get(local_name)
    }

    /// Given a parent complex type, find a child element definition by local name
    pub fn find_child_def<'a>(
        parent: &'a ComplexTypeDef,
        child_local_name: &str,
    ) -> Option<&'a ElementDef> {
        parent
            .children
            .iter()
            .find(|e| e.name.local_name == child_local_name)
    }
}
