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

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Adw, Gio

from .album_item import AlbumItem

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/album_view.ui')
class AlbumView(Gtk.ScrolledWindow):
    __gtype_name__ = 'album_view'

    __gsignals__ = {
        'done-loading': (GObject.SignalFlags.RUN_FIRST, None, ())
    }

    _title_label = Gtk.Template.Child()
    _subtitle_label = Gtk.Template.Child()
    _item_box = Gtk.Template.Child()
    _cover_image = Gtk.Template.Child()

    _menu_button = Gtk.Template.Child()
    _play_button = Gtk.Template.Child()

    _button_box = Gtk.Template.Child()
    _button2_box = Gtk.Template.Child()
    _cover_box = Gtk.Template.Child()
    _cover2_box = Gtk.Template.Child()
    _left_box = Gtk.Template.Child()

    _discs = {}

    _download_key = None
    _key = None

    _artist_view = False

    def __init__(self, artist_view=False, **kwargs):
        super().__init__(**kwargs)
        self._artist_view = artist_view
        if self._artist_view == True:
            self.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.NEVER)

    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("download-cover", self.__on_cover_downloaded)

    def change_album(self, key):
        self._title_label.set_text('')
        self._subtitle_label.set_text('')
        self._key = key
        while self._item_box.get_first_child() != None:
            self._item_box.remove(self._item_box.get_first_child())

        self._discs = {}

        self._connection_album_retrieved = self._plex.connect("album-retrieved", self.__album_retrieved)

        thread = threading.Thread(target=self._plex.get_album, args=(key,))
        thread.daemon = True
        thread.start()

    def __album_retrieved(self, plex, album, tracks):
        if self._key is not None and int(album.ratingKey) == int(self._key):
            self._plex.disconnect(self._connection_album_retrieved)
            GLib.idle_add(self.__album_process, album, tracks)

    def __album_process(self, album, tracks):
        self._item = album

        self._play_button.set_action_target_value(GLib.Variant.new_int64(int(self._item.ratingKey)))
        self._play_button.set_action_name("win.play-item")

        menu = Gio.Menu()
        if self._artist_view == False:
            menu_item = Gio.MenuItem.new(_("Go to artist view"), "win.show-artist-by-id")
            menu_item.set_action_and_target_value("win.show-artist-by-id", GLib.Variant.new_int64(int(self._item.parentRatingKey)))
            menu.append_item(menu_item)
        menu_item = Gio.MenuItem.new(_("Sync"), "win.sync-by-id")
        menu_item.set_action_and_target_value("win.sync-by-id", GLib.Variant.new_int64(self._item.ratingKey))
        menu.append_item(menu_item)
        menu_item = Gio.MenuItem.new(_("Shuffle"), "win.shuffle-by-id")
        menu_item.set_action_and_target_value("win.shuffle-by-id", GLib.Variant.new_int64(self._item.ratingKey))
        menu.append_item(menu_item)
        self._menu_button.set_menu_model(menu)

        self._title_label.set_text(album.title)
        if hasattr(album, "year"):
            self._subtitle_label.set_text(str(album.year))
        else:
            self._subtitle_label.set_text("")

        self._download_key = album.ratingKey
        self._download_thumb = album.thumb

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

        self._tracks = tracks
        self.__show_items()

    def __add_track(self, track, itemBox):
        itemBox.append(AlbumItem(self._plex, track))
        self._tracks.remove(track)

    def __show_items(self):
        while len(self._tracks) > 0:
            parentindex = '1'
            if self._tracks[0].parentIndex is not None:
                parentindex = self._tracks[0].parentIndex
            if parentindex not in self._discs:
                listBox = Gtk.ListBox()
                self._discs[parentindex] = listBox
                listBox.set_margin_bottom(10)
                listBox.set_visible(True)
                listBox.set_selection_mode(Gtk.SelectionMode(0))
                listBox.set_css_classes(["boxed-list"])
                listBox.connect("row-activated", self.__on_row_actived)

                label = Gtk.Label(label=_('Disc ') + str(parentindex))
                label.set_margin_bottom(10)
                label.set_visible(True)
                self._item_box.append(label)
                self._item_box.append(listBox)
            self.__add_track(self._tracks[0], self._discs[parentindex])
        if len(self._discs) == 1:
            label.set_visible(False)
        self.emit('done-loading')

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 150, 150)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)

    def __on_row_actived(self, widget, row):
        row.get_child().play_item()

    def width_changed(self, width):
        cover_box = self._cover_box
        button_box = self._button_box
        left_vissible = True
        if width < 750:
            cover_box = self._cover2_box
            button_box = self._button2_box
            left_vissible = False

        if self._play_button.get_parent() != button_box:
            self._play_button.unparent()
            self._play_button.set_parent(button_box)
            self._menu_button.unparent()
            self._menu_button.set_parent(button_box)
            self._left_box.set_visible(left_vissible)
        if self._cover_image.get_parent() != cover_box:
            self._cover_image.unparent()
            self._cover_image.set_parent(cover_box)
            
