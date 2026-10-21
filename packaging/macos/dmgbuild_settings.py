# dmgbuild settings. The macOS release job sets AXCENITY_* environment variables.
import os
from pathlib import Path

application = os.environ["AXCENITY_APP"]
icon = os.environ["AXCENITY_ICNS"]
background = os.environ["AXCENITY_DMG_BG"]
license_path = os.environ["AXCENITY_LICENSE"]
# `icon` is also dmgbuild's volume-icon setting; keep it as that path.

volume_name = "Axcenity Editor"
format = "UDZO"
files = [application]
symlinks = {"Applications": "/Applications"}
window_rect = ((200, 120), (720, 438))
icon_locations = {
    "Axcenity Editor.app": (180, 220),
    "Applications": (540, 220),
}
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
default_view = "icon-view"
arrange_by = None
grid_offset = (0, 0)
grid_spacing = 100
scroll_position = (0, 0)
label_pos = "bottom"
text_size = 12
icon_size = 128

license = {
    "default-language": "en_US",
    "licenses": {"en_US": Path(license_path).read_text(encoding="utf-8")},
    "buttons": {
        "en_US": (
            "English",
            "Agree",
            "Disagree",
            "Print",
            "Save",
            'If you agree with the terms of this license, click "Agree" to install. If you do not agree, click "Disagree".',
        )
    },
}
