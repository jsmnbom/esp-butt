---
aside: false
---

# Schematic

<SchematicViewer />

## Design Notes

### Power switch

S2 is a **DPDT (double pole, double throw)** slide switch used to cut power in two places simultaneously. One pole interrupts the VCC rail to all peripherals — the display, encoder, and sliders — so they draw no current when off. The other pole holds the ESP32-S3's **EN pin low**, which suspends the chip entirely rather than leaving it in an idle loop.

This is a pragmatic design choice that avoids idle current draw without requiring deep sleep firmware support. The trade-off is that it's a somewhat unconventional use of the EN pin. A cleaner future revision would use proper deep sleep mode combined with a P-channel MOSFET or a dedicated load switch IC on the battery rail, which would allow the firmware to participate in the power-down sequence and wake from user input.

### Battery

The controller is powered by a **3.7 V LiPo cell** (600 mAh recommended, see links in BOM) connected via a JST-PH connector (J2). There is no on-board charging circuit — the battery must be charged externally, for example with a standalone TP4056 module. This keeps the board simple and small, but means you'll need to disconnect the battery and charge it separately.
