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


@Gtk.Template(resource_path='/nl/g4d/Girens/media_box_video_top.ui')
class MediaBoxVideoTop(Gtk.Revealer):
    __gtype_name__ = 'media_box_video_top'

    _close_button = Gtk.Template.Child()
    _fullscreen_button = Gtk.Template.Child()
    _fullscreen_windowed_button = Gtk.Template.Child()

    _title_label = Gtk.Template.Child()
    _subtitle_label = Gtk.Template.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        
