// Copyright 2026 Ernesto Ruge
// Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

use std::path::Path;

use xml2json::schema::parser::parse_xsd;
use xml2json::converter::walker;

#[test]
fn test_basic_conversion() {
    let xsd_path = Path::new("tests/sample.xsd");
    let schema = parse_xsd(xsd_path).expect("Failed to parse XSD");

    let xml = std::fs::read_to_string("tests/sample.xml").expect("Failed to read XML");
    let json_str = walker::convert_to_string(&xml, &schema).expect("Failed to convert");

    let json: serde_json::Value = serde_json::from_str(&json_str).expect("Invalid JSON output");

    let catalog = &json["catalog"];

    // Name should be a string
    assert_eq!(catalog["name"], "Test Catalog");

    // Version attribute should be an integer
    assert_eq!(catalog["@version"], 2);

    // Items should always be an array (maxOccurs=unbounded)
    assert!(catalog["item"].is_array(), "item should be an array");
    let items = catalog["item"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    // First item checks
    let item0 = &items[0];
    assert_eq!(item0["title"], "Widget");
    assert_eq!(item0["@id"], 1);
    assert_eq!(item0["price"], 9.99);
    assert_eq!(item0["quantity"], 100);
    assert_eq!(item0["available"], true);

    // Tags should be an array (maxOccurs=unbounded)
    assert!(item0["tag"].is_array(), "tag should be an array");
    let tags = item0["tag"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0], "hardware");
    assert_eq!(tags[1], "sale");

    // Second item: no tags, but should still be absent (not an empty array)
    let item1 = &items[1];
    assert_eq!(item1["title"], "Gadget");
    assert_eq!(item1["available"], false);
}

#[test]
fn test_single_item_still_array() {
    let xsd_path = Path::new("tests/sample.xsd");
    let schema = parse_xsd(xsd_path).expect("Failed to parse XSD");

    let xml = r#"<?xml version="1.0"?>
    <catalog version="1">
        <name>Single</name>
        <item id="1">
            <title>Only One</title>
            <price>5.00</price>
            <quantity>1</quantity>
            <available>true</available>
            <tag>solo</tag>
        </item>
    </catalog>"#;

    let json_str = walker::convert_to_string(xml, &schema).expect("Failed to convert");
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("Invalid JSON");

    // Even with a single item, it should be an array
    assert!(json["catalog"]["item"].is_array());
    assert_eq!(json["catalog"]["item"].as_array().unwrap().len(), 1);

    // Single tag should also be an array
    assert!(json["catalog"]["item"][0]["tag"].is_array());
    assert_eq!(json["catalog"]["item"][0]["tag"].as_array().unwrap().len(), 1);
}

#[test]
fn test_extension_unbounded_with_xsi_type() {
    // Schema with inheritance: MaintenanceWorks extends Roadworks
    let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
    <xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
               xmlns:D2="urn:test"
               targetNamespace="urn:test">

        <xs:element name="root" type="D2:RootType"/>

        <xs:complexType name="RootType">
            <xs:sequence>
                <xs:element name="situation" type="D2:SituationRecord" minOccurs="0" maxOccurs="unbounded"/>
            </xs:sequence>
        </xs:complexType>

        <xs:complexType name="SituationRecord">
            <xs:sequence>
                <xs:element name="description" type="xs:string" minOccurs="0"/>
            </xs:sequence>
            <xs:attribute name="id" type="xs:string"/>
            <xs:attribute name="xsi:type" type="xs:string"/>
        </xs:complexType>

        <xs:complexType name="Roadworks">
            <xs:complexContent>
                <xs:extension base="D2:SituationRecord">
                    <xs:sequence>
                        <xs:element name="roadworksType" type="xs:string" minOccurs="0"/>
                    </xs:sequence>
                </xs:extension>
            </xs:complexContent>
        </xs:complexType>

        <xs:complexType name="MaintenanceWorks">
            <xs:complexContent>
                <xs:extension base="D2:Roadworks">
                    <xs:sequence>
                        <xs:element name="roadMaintenanceType" type="xs:string" minOccurs="1" maxOccurs="unbounded"/>
                    </xs:sequence>
                </xs:extension>
            </xs:complexContent>
        </xs:complexType>

    </xs:schema>"#;

    // XML that uses xsi:type to specify MaintenanceWorks
    let xml = r#"<?xml version="1.0"?>
    <root xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
        <situation id="123" xsi:type="MaintenanceWorks">
            <description>Some roadworks</description>
            <roadMaintenanceType>treeAndVegetationCuttingWork</roadMaintenanceType>
        </situation>
    </root>"#;

    let schema = xml2json::schema::parser::parse_xsd_from_str(xsd)
        .expect("Failed to parse XSD");
    let json_str = walker::convert_to_string(xml, &schema)
        .expect("Failed to convert");
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .expect("Invalid JSON");

    // situation should be an array (maxOccurs="unbounded")
    assert!(
        json["root"]["situation"].is_array(),
        "situation should be an array, got: {}",
        json["root"]["situation"]
    );

    let sit = &json["root"]["situation"][0];

    // roadMaintenanceType has maxOccurs="unbounded", so even a single value
    // should be wrapped in an array
    assert!(
        sit["roadMaintenanceType"].is_array(),
        "roadMaintenanceType should be an array, got: {}",
        sit["roadMaintenanceType"]
    );
    assert_eq!(
        sit["roadMaintenanceType"].as_array().unwrap(),
        &[serde_json::Value::String("treeAndVegetationCuttingWork".to_string())]
    );
}