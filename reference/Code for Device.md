// In your UDS/CAN handler
if (service == 0x27) {
    if (subfunc == 0x01) {  // Request Seed
        // Extract seed from response: 67 01 + seed[4]
        uint32_t seed = (response[2] << 24) | (response[3] << 16) |
                        (response[4] << 8) | response[5];

        uint32_t key = edc16c41_calculate_key(seed);

        // Send key: 27 02 + key[4]
        uint8_t key_msg[6] = {0x27, 0x02,
                              (uint8_t)(key >> 24), (uint8_t)(key >> 16),
                              (uint8_t)(key >> 8),  (uint8_t)key};
        send_uds_request(key_msg, 6);
    }
}
