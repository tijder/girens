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
from .gi_composites import GtkTemplate
from .cover_box import CoverBox

import threading

@GtkTemplate(ui='/nl/g4d/Girens/discover_view.ui')
class DiscoverView(Gtk.Box):
    __gtype_name__ = 'discover_view'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-album-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-artist-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _deck_shows_box = GtkTemplate.Child()
    _movies_shows_box = GtkTemplate.Child()
    _seasons_shows_box = GtkTemplate.Child()
    _music_shows_box = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._plex.connect("shows-latest", self.__on_show_latest_update)
        self._plex.connect("shows-deck", self.__on_show_deck_update)

    def refresh(self):
        for item in self._deck_shows_box.get_children():
            self._deck_shows_box.remove(item)

        for item in self._movies_shows_box.get_children():
            self._movies_shows_box.remove(item)

        for item in self._seasons_shows_box.get_children():
            self._seasons_shows_box.remove(item)

        for item in self._music_shows_box.get_children():
            self._music_shows_box.remove(item)

        thread = threading.Thread(target=self._plex.get_deck,)
        thread.daemon = True
        thread.start()

        thread = threading.Thread(target=self._plex.get_latest,)
        thread.daemon = True
        thread.start()

    def __on_show_latest_update(self, plex, items):
        for item in items:
            if(item.TYPE == 'movie'):
                GLib.idle_add(self.__add_to_hub, self._movies_shows_box, item)
            elif(item.TYPE == 'episode' or item.TYPE == 'season'):
                GLib.idle_add(self.__add_to_hub, self._seasons_shows_box, item)
            elif(item.TYPE == 'album'):
                GLib.idle_add(self.__add_to_hub, self._music_shows_box, item)

    def __on_show_deck_update(self, plex, items):
        for item in items:
            GLib.idle_add(self.__add_to_hub, self._deck_shows_box, item)

    def __add_to_hub(self, hub, item):
        cover = CoverBox(self._plex, item)
        cover.connect("view-show-wanted", self.__on_go_to_show_clicked)
        cover.connect("view-album-wanted", self.__on_go_to_album_clicked)
        cover.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
        hub.add(cover)

    def __on_go_to_show_clicked(self, cover, key):
        self.emit('view-show-wanted', key)

    def __on_go_to_album_clicked(self, cover, key):
        self.emit('view-album-wanted', key)

    def __on_go_to_artist_clicked(self, cover, key):
        self.emit('view-artist-wanted', key)

