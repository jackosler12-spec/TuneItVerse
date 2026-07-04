// In setupConnect() or the modal connect handler, add:

// Apply advanced CAN/ISO-TP settings before connecting
const applyAdvancedCanSettings = async () => {
  const canFdEnabled = document.getElementById('enable-can-fd')?.checked || false;
  const blockSize = parseInt(document.getElementById('iso-block-size')?.value || '0', 10);
  const stmin = parseInt(document.getElementById('iso-stmin')?.value || '5', 10);

  try {
    await invokeCmd('set_can_fd_mode', { enabled: canFdEnabled });
    await invokeCmd('set_iso_tp_parameters', { block_size: blockSize, stmin_ms: stmin });
    console.log(`[TuneItVerse] Applied CAN FD: ${canFdEnabled}, ISO-TP BS=${blockSize}, STmin=${stmin}ms`);
  } catch (e) {
    console.warn('Failed to apply advanced CAN settings:', e);
  }
};

// Call applyAdvancedCanSettings() just before the actual connect logic in btnModalConnect handler
// Example integration point:
// btnModalConnect?.addEventListener('click', async () => {
//   await applyAdvancedCanSettings();
//   // then existing connect code...
// });

// Also show/hide advanced section based on hardware type (optional polish)
const hwRadios = $$('input[name="hw-type"]');
hwRadios.forEach(radio => {
  radio.addEventListener('change', () => {
    const advSection = document.querySelector('#connect-modal .modal-body > div:last-child');
    if (advSection) {
      advSection.style.display = (radio.value === 'j2534') ? 'block' : 'block'; // always show for now
    }
  });
});