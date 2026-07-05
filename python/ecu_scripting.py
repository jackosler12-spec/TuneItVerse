# python/ecu_scripting.py — Deepened EDC16 support

# ... existing code ...

class ChecksumCalculator:
    # ... existing P01 + improved EDC16 ...

    def edc16_checksum(self, data: bytes, family: str = "EDC16C41") -> dict:
        """More realistic EDC16 checksum placeholder (expand with your reference offsets)."""
        if len(data) < 0x10000:
            return {"valid": False, "error": "Image too small"}

        # Example: sum-based check on common calibration blocks
        cal_sum = sum(data[0x0000:0xC000])
        expected = 0  # In real use, read from known CS location in the bin
        is_valid = (cal_sum & 0xFFFF) == (expected & 0xFFFF)

        return {
            "family": family,
            "valid": is_valid,
            "calculated": hex(cal_sum & 0xFFFF),
            "expected": hex(expected),
            "note": "Replace with exact Bosch algorithm from your reference dumps"
        }

# Add EDC16-specific map discovery helper
class MapDiscovery:
    def discover_edc16_maps(self, data: bytes):
        """Basic EDC16 map discovery (expand with real patterns from your bins)."""
        maps = []
        # Example patterns (real ones come from reverse engineering your 392203.bin etc.)
        if b'\x00\x00\x00\x00' in data[0x20000:0x30000]:
            maps.append({"name": "Torque Map", "addr": "0x22000", "size": "16x16"})
        if b'\xFF\xFF' in data[0x40000:0x50000]:
            maps.append({"name": "Boost Target", "addr": "0x48000", "size": "8x8"})
        return maps

# Expose via CLI if needed
# ... existing main block ...