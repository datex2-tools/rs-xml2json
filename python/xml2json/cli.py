# Copyright 2026 Ernesto Ruge
# Use of this source code is governed by an MIT-style license that can be found in the LICENSE.txt.

import sys
from xml2json import convert, convert_to_file


def main():
    if len(sys.argv) < 3 or len(sys.argv) > 4:
        print(f"Usage: {sys.argv[0]} <xml_file> <xsd_schema> [output_json]", file=sys.stderr)
        sys.exit(1)

    xml_path = sys.argv[1]
    xsd_path = sys.argv[2]

    if len(sys.argv) == 4:
        output_path = sys.argv[3]
        convert_to_file(xml_path, xsd_path, output_path)
    else:
        result = convert(xml_path, xsd_path)
        print(result)


if __name__ == "__main__":
    main()
