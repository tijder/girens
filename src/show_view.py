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

@GtkTemplate(ui='/nl/g4d/Girens/show_view.ui')
class ShowView(Gtk.Box):
    __gtype_name__ = 'show_view'

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()

    _play_button = GtkTemplate.Child()
    _shuffle_button = GtkTemplate.Child()

    _season_stack = GtkTemplate.Child()

    _cover_width = 200

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex

        self._plex.connect("shows-retrieved", self.__show_retrieved)

        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._shuffle_button.connect("clicked", self.__on_shuffle_button_clicked)

    def change_show(self, key):
        self._title_label.set_text('')
        self._subtitle_label.set_text('')
        for item in self._season_stack.get_children():
            self._season_stack.remove(item)

        thread = threading.Thread(target=self._plex.get_show, args=(key,))
        thread.daemon = True
        thread.start()

    def __show_retrieved(self, plex, show, episodes):
        self._show = show
        GLib.idle_add(self.__show_process, show, episodes)

    def __show_process(self, show, episodes):
        self._title_label.set_text(show.title)
        self._subtitle_label.set_text(str(show.year))

        seasons = {}

        for episode in episodes:
            if not episode.parentIndex in seasons:
                flow = Gtk.FlowBox()
                flow.set_valign(Gtk.Align.START)
                flow.set_halign(Gtk.Align.START)
                flow.set_max_children_per_line(30)
                flow.set_selection_mode(Gtk.SelectionMode.NONE)
                flow.set_homogeneous(True)
                seasons.update({episode.parentIndex : flow})
                self._season_stack.add_titled(flow, episode.parentIndex, episode.parentTitle)
                flow.show()
            self.__add_to_hub(seasons[episode.parentIndex], episode)

    def __on_show_deck_update(self, plex, items):
        for item in items:
            GLib.idle_add(self.__add_to_hub, self._deck_shows_box, item)

    def __add_to_hub(self, hub, item):
        cover = CoverBox(self._plex, item, show_view=True, cover_width=self._cover_width)
        hub.add(cover)

    def __on_play_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._show,))
        thread.daemon = True
        thread.start()

    def __on_shuffle_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._show,),kwargs={'shuffle':1})
        thread.daemon = True
        thread.start()

    def width_changed(self, width):
        if width < 450:
            self._cover_width = width / 2 - 10
        else:
            self._cover_width = 200
