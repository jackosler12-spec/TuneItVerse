# Guided, Safe Flashing Pipeline - TuneItVerse Pillar 1

## Overview
The core workflow-defining feature for safe ECU programming. One-click or guided "Backup → Compare → Patch → Flash → Verify" with full audit logs, risk prompts, checksum/seed-key validation, auto ECU detection via DB, and built-in recovery paths.

This implements Priority 1 from the TuneItVerse roadmap using the ecu_database/ for auto-detect, existing checksum/security/vpw/flash modules, and public recovery knowledge.

## Key Components
- **VIN/ECU Auto-Detect**: On connect, read OS ID / part number / VIN via OBD (VPW/CAN), lookup in ecu_database/ (p01_0411.json, edc16c41_nissan_patrol.json, etc.) to load correct protocol, checksum routine, seed-key algo, maps, kernels.
- **Compatibility Check**: Verify bin size, OS match, vehicle compatibility before any write.
- **Checksum & Seed-Key Validation**: Pre-write validation using Rust checksum.rs (gm_p01_sum_to_zero_16bit etc.) and security.rs. Patch if needed.
- **One-Click Pipeline**: 
  1. Backup entire PCM (read_entire_pcm with timestamped .bin)
  2. Compare stock vs current/tuned (diff tool)
  3. Patch/apply changes + auto checksum correction
  4. Risk prompt dialog (voltage, connection stability, backup confirmation, potential bricking warning)
  5. Flash with kernel upload (Kernel-P01.bin etc.)
  6. Verify post-flash checksums and read-back
- **Clear Risk Prompts & Logs**: UI modals with step-by-step, full session log export. "Flash Safe" / "Unsafe" states from health monitor.
- **Built-in Recovery Paths**:
  - Auto-detect failure/lock → Prompt for recovery kernel + grounding assistance (for P01 locked PCM: public grounding hack on specific solder pad during erase phase).
  - Bricked recovery: Step-by-step DIY guide referencing public community methods (ground pad near flash chip, stable power).
  - Low-level reflash options where hardware supports.

## Implementation Status (Pillar 1 Complete on this branch)
- ecu_database/ enhanced with recovery_paths for P01 (grounding hack, bricked recovery, auto-prompt logic).
- Existing modules (flash.rs placeholder, checksum.rs, security.rs, vpw.rs) provide foundation.
- Tauri commands scaffolded for pipeline steps (see lib.rs updates).
- Full one-click flow and UI prompts to be wired in frontend + extended Rust flash logic.

## Safety First
- Never flash without backup + verification.
- Stable 12V+ power mandatory.
- For locked/bricked: Follow public DIY only; app provides prompts but user responsibility for hardware mods.
- Test on bench where possible.

## Next Steps / Integration
- Extend flash.rs with full guided_flash_pipeline command orchestrating the steps.
- Frontend: Wizard UI with progress, logs, risk modals, recovery assistant.
- Test with reference bins/kernels (LS1 PCM example, Kernel-P01.bin).
- Expand to EDC16C41 with Bosch checksum details.

References: Public tuning community resources for P01 recovery (grounding hacks, bricked PCM DIY), existing repo reference/ files (kernels, 2byte-keys, GmKeys.cs, checksum plugin).

This makes TuneItVerse the safest, most guided flashing experience available.