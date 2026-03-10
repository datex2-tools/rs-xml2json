// Copyright 2026 Ernesto Ruge
// Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

use super::model::JsonType;

/// Maps an XSD built-in type local name to a JSON type.
/// See https://www.w3.org/TR/xmlschema-2/#built-in-datatypes
pub fn xsd_builtin_to_json(type_local_name: &str) -> Option<JsonType> {
    match type_local_name {
        // String types
        "string" | "normalizedString" | "token" | "language" | "Name" | "NCName" | "NMTOKEN"
        | "NMTOKENS" | "ID" | "IDREF" | "IDREFS" | "ENTITY" | "ENTITIES" | "QName"
        | "NOTATION" | "anyURI" => Some(JsonType::String),

        // Date/time types (represented as strings in JSON)
        "date" | "dateTime" | "time" | "gYear" | "gYearMonth" | "gMonth" | "gMonthDay"
        | "gDay" | "duration" => Some(JsonType::String),

        // Integer types
        "integer" | "nonPositiveInteger" | "negativeInteger" | "long" | "int" | "short"
        | "byte" | "nonNegativeInteger" | "unsignedLong" | "unsignedInt" | "unsignedShort"
        | "unsignedByte" | "positiveInteger" => Some(JsonType::Integer),

        // Float types
        "float" | "double" | "decimal" => Some(JsonType::Float),

        // Boolean
        "boolean" => Some(JsonType::Boolean),

        // Binary (represented as strings in JSON)
        "base64Binary" | "hexBinary" => Some(JsonType::String),

        // anySimpleType / anyType — treat as string
        "anySimpleType" => Some(JsonType::String),
        "anyType" => None, // could be complex or simple, caller must decide

        _ => None,
    }
}
