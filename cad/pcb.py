from pathlib import Path

import cad_utils

PCB_NAME = "esp-butt"
REPO_ROOT = Path(__file__).parent.parent
DOCS_PUBLIC = REPO_ROOT / "docs" / "public"
MODELS_DIR = DOCS_PUBLIC / "models"
PCB_DIR = REPO_ROOT / "pcb"
PCB_EXPORT_DIR = PCB_DIR / "export"

PCB = cad_utils.load_pcb(
  PCB_NAME,
  PCB_DIR / PCB_NAME,
  PCB_EXPORT_DIR,
  {"AZ-Delivery_-_OLED13_IIC": "J1 Screen"},
  {
    "J1 Screen": "black",
    "R7": "silver",
    "R8": "silver",
  },
)
