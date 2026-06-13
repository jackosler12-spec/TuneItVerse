  try {
    // Pass raw bytes — Rust: validate_bin(file_bytes: Vec<u8>)
    const result = await invokeCmd("validate_bin", {
      fileBytes: Array.from(state.selectedFileBytes),
    });
    if (!result) throw new Error("No validation result returned.");
    $("#bin-osid").textContent = result.detected_os_id || "Unknown";
    $("#bin-checksum").textContent = result.checksum_ok ? "OK" : "Failed";
    $("#bin-compat").textContent = result.compatibility || "Unknown";
    state.binValidated = !!result.checksum_ok;
    state.binCompatible = !!result.compatible;
    updateChecklist();
    logJob(`BIN validated. OSID=${result.detected_os_id}, checksum=${result.checksum_ok}, compat=${result.compatibility}`);

    // Detailed checksum verification for ECU dumps / BINs (now with full report)
    if (result.checksum_report) {
      const r = result.checksum_report;
      const detailMsg = `Checksum details: ${r.valid_count} valid / ${r.fixed_count} fixed / ${r.failed_count} failed regions. All valid: ${r.all_valid}`;
      logJob(detailMsg);
      if (!r.all_valid) {
        const badRegions = r.regions
          .filter(reg => !reg.was_valid)
          .map(reg => `${reg.name} (block ${reg.block})`)
          .join(", ");
        if (badRegions) logJob(`Invalid regions requiring attention: ${badRegions}`);
      }
      // Color the checksum field based on detailed result
      const csEl = $("#bin-checksum");
      if (csEl) {
        csEl.style.color = r.all_valid ? "var(--success)" : "var(--danger)";
      }
    }

    setJobPhase("Idle");
  } catch (err) {
    setJobPhase("Failed");
    logJob(`BIN validation failed: ${err}`);
    alert(`BIN validation failed: ${err}`);
  }