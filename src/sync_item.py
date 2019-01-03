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

import cairo
import threading

@GtkTemplate(ui='/nl/g4d/Girens/sync_item.ui')
class SyncItem(Gtk.Box):
    __gtype_name__ = 'sync_item'

    _remove_button = GtkTemplate.Child()
    _title_label = GtkTemplate.Child()

    def __init__(self, plex, item_dict, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._item_dict = item_dict

        self._plex.connect('item-retrieved', self.__on_item_retrieved)
        self._remove_button.connect('clicked', self.__on_remove_clicked)

        thread = threading.Thread(target=self._plex.retrieve_item, args=(self._item_dict['rating_key'],))
        thread.daemon = True
        thread.start()

    def __on_item_retrieved(self, plex, item):
        if (int(item.ratingKey) == int(self._item_dict['rating_key'])):
            self._item = item
            if (item.TYPE == 'movie' or item.TYPE == 'episode'):
                self._title_label.set_text(item._prettyfilename())
            elif (item.TYPE == 'album' or item.TYPE == 'playlist' or item.TYPE == 'artist'):
                self._title_label.set_text(item.title)

    def __on_remove_clicked(self, button):
        self._plex.remove_from_sync(self._item_dict['rating_key'])
        
