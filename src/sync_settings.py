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


import cairo
import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/sync_settings.ui')
class SyncSettings(Gtk.Window):
    __gtype_name__ = 'sync_settings'

    _convert_button = Gtk.Template.Child()
    _unwatched_button = Gtk.Template.Child()
    _max_items_entry = Gtk.Template.Child()

    _sync_button = Gtk.Template.Child()

    def __init__(self, plex, item, **kwargs):
        super().__init__(**kwargs)

        self._plex = plex
        self._item = item

        self._sync_button.connect('clicked', self.__on_sync_clicked)

    def __on_sync_clicked(self, button):
        thread = threading.Thread(target=self._plex.add_to_sync, args=(self._item,),kwargs={'converted':self._convert_button.get_active(),'max_items':int(self._max_items_entry.get_text()), 'only_unwatched':self._unwatched_button.get_active()})
        thread.daemon = True
        thread.start()
        self.destroy()
        
