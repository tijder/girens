# main.py
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

import sys
import gi

gi.require_version('Gtk', '3.0')
gi.require_version('Handy', '0.0')

from gi.repository import Gtk, Gio, GObject, Handy, GLib

from .window import PlexWindow


class Application(Gtk.Application):
    def __init__(self):
        super().__init__(application_id='nl.g4d.Girens',
                         flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE)
        GLib.set_application_name("Girens")
        GLib.set_prgname("girens")
        self.add_main_option("show-id", b"s", GLib.OptionFlags.NONE,
                             GLib.OptionArg.STRING, "Show id", None)
        self.connect("command-line", self.__on_command_line)
        self.register(None)

    def __on_command_line(self, app, app_cmd_line):
        """
            Handle command line
            @param app as Gio.Application
            @param options as Gio.ApplicationCommandLine
        """
        params=None
        try:
            args = app_cmd_line.get_arguments()
            options = app_cmd_line.get_options_dict()
            if options.contains("show-id"):
                value = options.lookup_value("show-id").get_string()
                params = value.split(";")
        except Exception as e:
            print("Application::__on_command_line(): %s", e)
        self.do_activate(show_id=params)
        return 0


    def do_activate(self, show_id=None):
        GObject.type_register(Handy.Leaflet)
        win = self.props.active_window
        if not win:
            win = PlexWindow(application=self, show_id=show_id)
        else:
            win.show_by_id(show_id)
        win.present()


def main(version):
    app = Application()
    return app.run(sys.argv)
