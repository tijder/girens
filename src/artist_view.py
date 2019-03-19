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
from .album_view import AlbumView

import threading

@GtkTemplate(ui='/nl/g4d/Girens/artist_view.ui')
class ArtistView(Gtk.Box):
    __gtype_name__ = 'artist_view'

    _title_label = GtkTemplate.Child()
    _album_box = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

        self._plex.connect("artist-retrieved", self.__artist_retrieved)

    def change_artist(self, key):
        self._title_label.set_text('')
        for item in self._album_box.get_children():
            self._album_box.remove(item)

        thread = threading.Thread(target=self._plex.get_artist, args=(key,))
        thread.daemon = True
        thread.start()

    def __artist_retrieved(self, plex, artist, albums):
        GLib.idle_add(self.__artist_process, artist, albums)

    def __artist_process(self, artist, albums):
        self._title_label.set_text(artist.title)

        for album in albums:
            album_view = AlbumView(self._plex)
            album_view.change_album(album.ratingKey)
            self._album_box.add(album_view)
