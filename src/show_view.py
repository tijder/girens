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


from .cover_box import CoverBox
from .show_view_item import ShowViewItem

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/show_view.ui')
class ShowView(Gtk.ScrolledWindow):
    __gtype_name__ = 'show_view'

    _title_label = Gtk.Template.Child()
    _subtitle_label = Gtk.Template.Child()
    _description_label = Gtk.Template.Child()

    _cover_image = Gtk.Template.Child()

    _play_button = Gtk.Template.Child()
    _shuffle_button = Gtk.Template.Child()

    _season_stack = Gtk.Template.Child()
    _season_scrolled_window = Gtk.Template.Child()

    _cover_width = 200

    _download_key = None

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._shuffle_button.connect("clicked", self.__on_shuffle_button_clicked)


    def set_plex(self, plex):
        self._plex = plex

        self._plex.connect("shows-retrieved", self.__show_retrieved)
        self._plex.connect("download-cover", self.__on_cover_downloaded)

    def change_show(self, key):
        self._title_label.set_text('')
        self._subtitle_label.set_text('')
        self._description_label.set_text('')
        self._cover_image.set_filename(None)

        while self._season_stack.get_first_child() != None:
            self._season_stack.remove(self._season_stack.get_first_child())

        thread = threading.Thread(target=self._plex.get_show, args=(key,))
        thread.daemon = True
        thread.start()

    def __show_retrieved(self, plex, show, episodes):
        self._show = show
        GLib.idle_add(self.__show_process, show, episodes)

    def __show_process(self, show, episodes):
        self._title_label.set_text(show.title)
        self._subtitle_label.set_text(str(show.year))
        self._description_label.set_text(show.summary)

        self._download_key = show.ratingKey
        self._download_thumb = show.thumb

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

        seasons = {}

        for episode in episodes:
            if not episode.parentIndex in seasons:
                listBox = Gtk.ListBox()
                listBox.set_selection_mode(Gtk.SelectionMode.NONE)
                listBox.set_css_classes(["boxed-list"])
                listBox.connect("row-activated", self.__on_row_actived)
                seasons.update({episode.parentIndex : listBox})
                self._season_stack.add_titled(listBox, episode.parentIndex, episode.parentTitle)
            self.__add_to_hub(seasons[episode.parentIndex], episode)

        if len(seasons) == 1:
            self._season_scrolled_window.set_visible(False)
        else:
            self._season_scrolled_window.set_visible(True)

    def __on_show_deck_update(self, plex, items):
        for item in items:
            GLib.idle_add(self.__add_to_hub, self._deck_shows_box, item)

    def __add_to_hub(self, hub, item):
        show_item = ShowViewItem()
        show_item.set_plex(self._plex)
        show_item.set_item(item)
        hub.append(show_item)

    def __on_play_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._show,))
        thread.daemon = True
        thread.start()

    def __on_shuffle_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._show,),kwargs={'shuffle':1})
        thread.daemon = True
        thread.start()

    def __on_row_actived(self, widget, row):
        row.get_child().play_item()

    def width_changed(self, width):
        if width < 450:
            self._cover_width = width / 2 - 10
        else:
            self._cover_width = 200

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            GLib.idle_add(self.__set_image, path)

    def __set_image(self, pix):
        self._cover_image.set_filename(pix)
