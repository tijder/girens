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

from gi.repository import Gtk, GLib
from .gi_composites import GtkTemplate
from .cover_box import CoverBox

import threading

@GtkTemplate(ui='/org/gnome/Plex/discover_view.ui')
class DiscoverView(Gtk.Box):
    __gtype_name__ = 'discover_view'

    _label = GtkTemplate.Child()
    _label2 = GtkTemplate.Child()

    _deck_shows_box = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._plex.connect("shows-latest", self.__on_show_latest_update)
        self._plex.connect("shows-deck", self.__on_show_deck_update)

    def refresh(self):
        self._label2.set_text(self._plex._account.username)

        thread = threading.Thread(target=self._plex.get_deck,)
        thread.daemon = True
        thread.start()

    def __on_show_latest_update(self, plex, shows):
        print(shows)

    def __on_show_deck_update(self, plex, shows):
        GLib.idle_add(self.__add_to_hub, self._deck_shows_box, shows)

    def __add_to_hub(self, hub, shows):
        for show in shows:
            cover = CoverBox(show)
            hub.add(cover)
