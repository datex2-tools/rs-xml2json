# xml2json

A high-performance XML-to-JSON converter written in Rust with Python bindings. It uses XSD schema definitions to produce correctly typed JSON output — integers, floats, booleans, and arrays are represented as their proper JSON types rather than treating everything as strings.

## Features

- **Schema-aware conversion**: Uses XSD schemas to determine JSON types (string, integer, float, boolean) and array structures (via `maxOccurs`)
- **Correct array handling**: Elements with `maxOccurs="unbounded"` are always emitted as JSON arrays, even when only a single element is present
- **Attribute support**: XML attributes are mapped to JSON keys prefixed with `@`
- **Streaming file I/O**: Converts large XML files with buffered reading/writing (8 MB buffers)
- **Pre-parsed schemas**: Parse an XSD once, reuse it across multiple conversions
- **XSD import/include support**: Automatically resolves and parses referenced schema files
- **`xsi:type` support**: Overrides element types dynamically based on `xsi:type` attributes
- **Python bindings via PyO3**: Use directly from Python as a native module

## Requirements

- Rust (edition 2021)
- Python >= 3.8
- [maturin](https://github.com/PyO3/maturin) (for building the Python package)
- [uv](https://github.com/astral-sh/uv) (optional, for managing the Python environment)

## Installation

### Build and install the Python package

```bash
# Using maturin directly
maturin develop --release

# Or using uv
uv pip install -e .
```

This compiles the Rust code and installs the `xml2json` Python module.

### Build a wheel using Docker

A Docker-based build produces a self-contained `.whl` file for your local architecture without needing Rust or maturin installed on the host. By default it matches your system Python version (e.g. 3.12, 3.13, 3.14).

Two build variants are available:

| Variant | Dockerfile | Wheel type | Use case |
|---|---|---|---|
| `debian` (default) | `Dockerfile.debian` | `manylinux` | Standard glibc-based distros (Ubuntu, Debian, Fedora, ...) |
| `alpine` | `Dockerfile.alpine` | `musllinux` | Alpine-based / musl environments |

```bash
# Build a manylinux wheel matching your system Python version (output goes to dist/)
make build

# Build a musllinux wheel instead
make build VARIANT=alpine

# Build for a specific Python version
make build PYTHON_VERSION=3.13
make build PYTHON_VERSION=3.14

# Combine both options
make build VARIANT=alpine PYTHON_VERSION=3.14
```

The resulting wheel will be in the `dist/` directory. To build and install into a local `.venv` in one step:

```bash
make install
```

This creates the venv if it doesn't exist, builds the wheel, and installs it.

### Build as a Rust library only

```bash
cargo build --release
```

## Usage

### Python API

```python
from xml2json import Schema, convert, convert_to_file, convert_bytes, \
    convert_with_schema, convert_to_file_with_schema, convert_bytes_with_schema
```

#### One-shot conversion (parses XSD each time)

```python
# Convert XML file to JSON string
json_string = convert("data.xml", "schema.xsd")

# Convert XML file to JSON file (streaming, low memory)
convert_to_file("data.xml", "schema.xsd", "output.json")

# Convert raw bytes
json_string = convert_bytes(xml_bytes, xsd_bytes)
```

#### Pre-parsed schema (recommended for multiple conversions)

```python
from xml2json import Schema

# Parse the schema once
schema = Schema.from_file("schema.xsd")
# Or from bytes
schema = Schema.from_bytes(xsd_bytes)

# Reuse across conversions
json_string = convert_with_schema("data.xml", schema)
convert_to_file_with_schema("data.xml", schema, "output.json")
json_string = convert_bytes_with_schema(xml_bytes, schema)
```

### Example

Given this XSD schema:

```xml
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:element name="catalog" type="CatalogType"/>
    <xs:complexType name="CatalogType">
        <xs:sequence>
            <xs:element name="name" type="xs:string"/>
            <xs:element name="item" type="ItemType" minOccurs="0" maxOccurs="unbounded"/>
        </xs:sequence>
        <xs:attribute name="version" type="xs:integer"/>
    </xs:complexType>
    <xs:complexType name="ItemType">
        <xs:sequence>
            <xs:element name="title" type="xs:string"/>
            <xs:element name="price" type="xs:decimal"/>
            <xs:element name="quantity" type="xs:integer"/>
            <xs:element name="available" type="xs:boolean"/>
        </xs:sequence>
        <xs:attribute name="id" type="xs:integer"/>
    </xs:complexType>
</xs:schema>
```

And this XML:

```xml
<catalog version="2">
    <name>Test Catalog</name>
    <item id="1">
        <title>Widget</title>
        <price>9.99</price>
        <quantity>100</quantity>
        <available>true</available>
    </item>
</catalog>
```

The output JSON will be:

```json
{
  "catalog": {
    "@version": 2,
    "name": "Test Catalog",
    "item": [
      {
        "@id": 1,
        "title": "Widget",
        "price": 9.99,
        "quantity": 100,
        "available": true
      }
    ]
  }
}
```

Note that `item` is always an array (because the schema declares `maxOccurs="unbounded"`), and numeric/boolean values are properly typed.

## JSON mapping conventions

| XML construct | JSON representation |
|---|---|
| Element with simple type | Value (string, number, boolean) |
| Element with complex type | Object |
| Element with `maxOccurs > 1` | Array (even with a single item) |
| Attribute | Key prefixed with `@` (e.g., `@id`) |
| Mixed content text | `_text` key |
| Empty element | `null` |

## Running tests

```bash
cargo test
```

## Project structure

```
├── Cargo.toml              # Rust package manifest
├── pyproject.toml           # Python package manifest (maturin)
├── src/
│   ├── lib.rs              # Python bindings (PyO3 module)
│   ├── error.rs            # Error types
│   ├── schema/
│   │   ├── mod.rs
│   │   ├── model.rs        # Schema data model (ElementDef, TypeDef, etc.)
│   │   ├── parser.rs       # XSD parser
│   │   └── type_map.rs     # XSD built-in type → JSON type mapping
│   └── converter/
│       ├── mod.rs
│       └── walker.rs       # XML → JSON conversion engine
├── python/
│   └── xml2json/
│       └── __init__.py     # Python package re-exports
├── tests/
│   ├── integration_test.rs # Rust integration tests
│   ├── sample.xsd          # Test schema
│   └── sample.xml          # Test data
└── data/
    └── schema.xsd          # Example schema
```

## License

See the project license file for details.
