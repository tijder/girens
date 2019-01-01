# cover_box.py
#
# Copyright 2018 Gerben Droogers
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Gdk
from .gi_composites import GtkTemplate

from .download_row import DownloadRow

import cairo
import threading

@GtkTemplate(ui='/nl/g4d/Girens/download_menu.ui')
class DownloadMenu(Gtk.Popover):
    __gtype_name__ = 'download_menu'

    __gsignals__ = {
        'show-button': (GObject.SignalFlags.RUN_FIRST, None, ()),
    }

    _item_box = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

        self._plex.connect("item-downloading", self.__on_item_downloading)

    def __on_item_downloading(self, plex, item, status):
        if (status != False):
            self.emit('show-button')
            self._item_box.add(DownloadRow(self._plex, item))
