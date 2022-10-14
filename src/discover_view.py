# window.py
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

from .cover_box import CoverBox
from .list import List
from .item_bin import ItemBin

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/discover_view.ui')
class DiscoverView(Gtk.ScrolledWindow):
    __gtype_name__ = 'discover_view'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-album-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-artist-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _deck_shows_box = Gtk.Template.Child()
    _movies_shows_box = Gtk.Template.Child()
    _seasons_shows_box = Gtk.Template.Child()
    _music_shows_box = Gtk.Template.Child()

    _cover_width = 200

    def __init__(self, **kwargs):
        super().__init__(**kwargs)


    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("shows-latest", self.__on_show_latest_update)
        self._plex.connect("shows-deck", self.__on_show_deck_update)

        self._deck_shows_box.set_plex(plex)
        self._movies_shows_box.set_plex(plex)
        self._seasons_shows_box.set_plex(plex)
        self._music_shows_box.set_plex(plex)

    def refresh(self):
        self._deck_shows_box.empty_list()
        self._movies_shows_box.empty_list()
        self._seasons_shows_box.empty_list()
        self._music_shows_box.empty_list()

        thread = threading.Thread(target=self._plex.get_deck,)
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.get_latest,)
        thread.daemon = True
        thread.start()

    def __on_show_latest_update(self, plex, items):
        for item in items:
            if(item.TYPE == 'movie'):
                GLib.idle_add(self.__add_to_list, self._movies_shows_box, item)
            elif(item.TYPE == 'episode' or item.TYPE == 'season'):
                GLib.idle_add(self.__add_to_list, self._seasons_shows_box, item)
            elif(item.TYPE == 'album'):
                GLib.idle_add(self.__add_to_list, self._music_shows_box, item)

    def __on_show_deck_update(self, plex, items):
        for item in items:
            GLib.idle_add(self.__add_to_list, self._deck_shows_box, item)

    def __add_to_list(self, hub, item):
        item_bin = ItemBin()
        item_bin.set_item(item)
        hub.add_item(item_bin)

    def __add_to_hub(self, hub, item):
        cover = CoverBox(self._plex, cover_width=self._cover_width)
        cover.set_item(item)
        cover.connect("view-show-wanted", self.__on_go_to_show_clicked)
        cover.connect("view-album-wanted", self.__on_go_to_album_clicked)
        cover.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
        hub.append(cover)

    def __on_go_to_show_clicked(self, cover, key):
        self.emit('view-show-wanted', key)

    def __on_go_to_album_clicked(self, cover, key):
        self.emit('view-album-wanted', key)

    def __on_go_to_artist_clicked(self, cover, key):
        self.emit('view-artist-wanted', key)

    def width_changed(self, width):
        if width < 450:
            self._cover_width = width / 2 - 10
        else:
            self._cover_width = 200
            
