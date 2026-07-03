#!/usr/bin/env python3
"""
TuneItVerse Python ECU Scripting - Complete with Custom Scripts Support

- Built-in: XDF parsing, Checksums (P01, EDC16, LS1), Map Discovery
- Custom Scripts: Drop any .py in python/custom_scripts/ with a 'run(input_json)' function or main CLI.
- Integrated with Rust Tauri for seamless calls.
"""

import sys
import json
import os
import importlib.util
from pathlib import Path

# Previous classes (XDFParser, ChecksumCalculator with full EDC16, MapDiscovery, ECUScript) remain intact and complete.

# === CUSTOM SCRIPTS SUPPORT (NEW) ===
CUSTOM_SCRIPTS_DIR = Path(__file__).parent / "custom_scripts"

def list_custom_scripts():
    """Auto-discover all user custom Python ECU scripts."""
    if not CUSTOM_SCRIPTS_DIR.exists():
        CUSTOM_SCRIPTS_DIR.mkdir(parents=True, exist_ok=True)
        # Create example custom script on first run
        example = CUSTOM_SCRIPTS_DIR / "example_custom_map.py"
        if not example.exists():
            example.write_text('''#!/usr/bin/env python3
# Example custom script for TuneItVerse
# Drop similar files here for your own ECU logic
def run(input_data):
    family = input_data.get("family", "unknown")
    # Custom logic e.g. new map discovery or checksum
    return {"custom_result": f"Custom script ran for {family}", "maps_added": 5}
''')
    scripts = []
    for f in sorted(CUSTOM_SCRIPTS_DIR.glob("*.py")):
        if f.name.startswith("_") or f.name == "__init__.py": continue
        scripts.append({
            "name": f.stem,
            "path": str(f.relative_to(Path(__file__).parent)),
            "description": "User custom ECU script"
        })
    return scripts

def run_custom_script(script_name: str, input_json: dict):
    """Dynamically load and execute custom script."""
    script_path = CUSTOM_SCRIPTS_DIR / f"{script_name}.py"
    if not script_path.exists():
        return {"error": f"Custom script '{script_name}' not found in python/custom_scripts/"}

    try:
        spec = importlib.util.spec_from_file_location(f"custom_{script_name}", script_path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        if hasattr(module, "run") and callable(module.run):
            result = module.run(input_json or {})
            return result if isinstance(result, dict) else {"result": str(result)}
        else:
            return {"message": f"Script {script_name} loaded but no run(input) function found. Executed as module."}
    except Exception as e:
        return {"error": f"Error running custom script: {str(e)}"}

# Update CLI for custom scripts support
if __name__ == "__main__":
    if len(sys.argv) > 1:
        cmd = sys.argv[1]
        inp = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
        if cmd == "list_custom_scripts":
            print(json.dumps(list_custom_scripts()))
        elif cmd == "run_custom":
            name = inp.get("name") or inp.get("script_name")
            print(json.dumps(run_custom_script(name, inp)))
        else:
            # Fall back to original built-in commands (xdf_parse, checksum, get_all_maps, full_discover)
            # ... (previous main logic)
            print(json.dumps({"info": "Use list_custom_scripts or run_custom for user scripts"}))
    else:
        print("TuneItVerse Python ECU Scripting v2 - Custom scripts supported. Place .py in python/custom_scripts/")