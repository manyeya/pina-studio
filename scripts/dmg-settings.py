# Pina Studio's disk image, for dmgbuild (scripts/release-mac.sh runs it): one thing in it, "Install
# Pina Studio", in the middle of a 640 by 400 window over scripts/dmg-background.py's picture; no
# toolbar, sidebar or status bar; Pina Studio's icon on the volume. The picture and what goes in come
# from release-mac.sh (-D installer=… background=… volicon=…).
import os

installer = defines["installer"]
files = [installer]
icon_locations = {os.path.basename(installer): (320, 168)}
# No hidden-extension flag: it's written onto the app itself and breaks its signature (the
# Finder hides ".app" anyway).
background = defines["background"]
icon = defines["volicon"]
window_rect = ((200, 140), (640, 400))
default_view = "icon-view"
show_toolbar = False
show_sidebar = False
show_status_bar = False
show_tab_view = False
show_pathbar = False
include_icon_view_settings = True
icon_size = 128
text_size = 13
arrange_by = None
format = "ULFO"
filesystem = "HFS+"
