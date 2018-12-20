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

from gi.repository import Gtk, GLib, GObject
from .gi_composites import GtkTemplate

from .cover_box import CoverBox

import threading

@GtkTemplate(ui='/org/gnome/Plex/search_view.ui')
class SearchView(Gtk.Box):
    __gtype_name__ = 'search_view'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _title_label = GtkTemplate.Child()
    _section_flow = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._plex.connect("search-item-retrieved", self.__on_search_items_retrieved)


    def refresh(self, search):
        self._search = search

        self._title_label.set_label(self._search)

        for item in self._section_flow.get_children():
            self._section_flow.remove(item)

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'movie'})
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'show'})
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'episode'})
        thread.daemon = True
        thread.start()

    def __on_search_items_retrieved(self, plex, search, items):
        if (self._search == search):
            GLib.idle_add(self.__process_search_items, items)

    def __process_search_items(self, items):
        for item in items:
            if (item.TYPE == 'episode' or item.TYPE == 'season' or item.TYPE == 'show' or item.TYPE == 'movie'):
                cover = CoverBox(self._plex, item)
                cover.connect("view-show-wanted", self.__on_go_to_show_clicked)
                self._section_flow.add(cover)

    def __on_go_to_show_clicked(self, cover, key):
        self.emit('view-show-wanted', key)
