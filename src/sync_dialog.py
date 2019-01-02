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
from .sync_item import SyncItem

import cairo
import threading

@GtkTemplate(ui='/nl/g4d/Girens/sync_dialog.ui')
class SyncDialog(Gtk.Dialog):
    __gtype_name__ = 'sync_dialog'

    _item_box = GtkTemplate.Child()
    _sync_button = GtkTemplate.Child()
    _ok_button = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

        self._sync_button.connect("clicked", self.__on_sync_clicked)
        self._ok_button.connect("clicked", self.__on_ok_clicked)
        self._plex.connect("sync-items", self.__on_sync_items_retrieved)

        self._plex.get_sync_items()


    def __on_sync_items_retrieved(self, plex, items):
        for item in self._item_box.get_children():
            self._item_box.remove(item)

        for item_keys in items:
            sync_item = SyncItem(self._plex, items[item_keys])
            self._item_box.add(sync_item)

    def __on_ok_clicked(self, button):
        self.destroy()

    def __on_sync_clicked(self, button):
        thread = threading.Thread(target=self._plex.sync)
        thread.daemon = True
        thread.start()
        self.destroy()
