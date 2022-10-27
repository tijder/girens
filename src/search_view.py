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


from .item_bin import ItemBin

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/search_view.ui')
class SearchView(Gtk.Box):
    __gtype_name__ = 'search_view'

    _title_label = Gtk.Template.Child()
    _section_flow = Gtk.Template.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

    def set_plex(self, plex):
        self._plex = plex
        self._section_flow.set_plex(plex)
        self._section_flow.set_grid_mode()
        self._plex.connect("search-item-retrieved", self.__on_search_items_retrieved)


    def refresh(self, search):
        self._search = search

        self._title_label.set_label(self._search)

        self._section_flow.empty_list()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'movie'})
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'show'})
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'episode'})
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'season'})
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'artist'})
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.search_library, args=(self._search,),kwargs={'libtype':'album'})
        thread.daemon = True
        thread.start()

    def __on_search_items_retrieved(self, plex, search, items):
        if (self._search == search):
            GLib.idle_add(self.__process_search_items, items)

    def __process_search_items(self, items):
        for item in items:
            if (item.TYPE in ['episode', 'season', 'show', 'movie', 'artist', 'album']):
                item_bin = ItemBin()
                item_bin.set_item(item)
                self._section_flow.add_item(item_bin)
