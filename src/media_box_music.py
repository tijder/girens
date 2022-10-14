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

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Gdk

from .cover_box import CoverBox
from .playqueue_popover import PlayqueuePopover

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/media_box_music.ui')
class MediaBoxMusic(Gtk.Revealer):
    __gtype_name__ = 'media_box_music'

    _close_button = Gtk.Template.Child()
    _play_button = Gtk.Template.Child()
    _prev_button = Gtk.Template.Child()
    _next_button = Gtk.Template.Child()
    _play_image = Gtk.Template.Child()

    _media_settings = Gtk.Template.Child()

    _title_label = Gtk.Template.Child()
    _subtitle_label = Gtk.Template.Child()
    _scale_bar = Gtk.Template.Child()
    _scale_adjustment = Gtk.Template.Child()

    _playqueue_button = Gtk.Template.Child()
    _cover_image = Gtk.Template.Child()

    _time_left_label = Gtk.Template.Child()
    _time_right_label = Gtk.Template.Child()

    _label_box = Gtk.Template.Child()
    _label2_box = Gtk.Template.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        

    def width_changed(self, width):
        box = self._label_box
        if width < 550:
            box = self._label2_box

        if self._title_label.get_parent() != box:
            self._title_label.reparent(box)
            self._subtitle_label.reparent(box)

