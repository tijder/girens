from gi.repository import GObject, Gio, GLib, Gtk

class MediaKeys(GObject.GObject):
    """Media keys handling for Media
    """

    __gtype_name__ = 'MediaKeys'

    def __repr__(self):
        return '<MediaKeys>'

    def __init__(self, player, window):
        """Initialize media keys handling

        :param Player player: Player object
        :param Gtk.Window window: Window to grab keys if focused
        """
        super().__init__()

        self._player = player
        self._window = window

        self._media_keys_proxy = None

        self._init_media_keys_proxy()

    def _init_media_keys_proxy(self):
        def name_appeared(connection, name, name_owner, data=None):
            Gio.DBusProxy.new_for_bus(
                Gio.BusType.SESSION,
                Gio.DBusProxyFlags.DO_NOT_LOAD_PROPERTIES, None,
                "org.gnome.SettingsDaemon.MediaKeys",
                "/org/gnome/SettingsDaemon/MediaKeys",
                "org.gnome.SettingsDaemon.MediaKeys", None,
                self._media_keys_proxy_ready)

        Gio.bus_watch_name(
            Gio.BusType.SESSION, "org.gnome.SettingsDaemon.MediaKeys",
            Gio.BusNameWatcherFlags.NONE, name_appeared, None)

    def _media_keys_proxy_ready(self, proxy, result, data=None):
        try:
            self._media_keys_proxy = proxy.new_finish(result)
        except GLib.Error as e:
            print(
                "Error: Failed to contact settings daemon:", e.message)
            return

        self._media_keys_proxy.connect("g-signal", self._handle_media_keys)

        self._ctrlr = Gtk.EventControllerKey().new(self._window)
        self._ctrlr.props.propagation_phase = Gtk.PropagationPhase.CAPTURE
        self._ctrlr.connect("focus-in", self._grab_media_player_keys)

    def _grab_media_player_keys(self, controllerkey=None):
        def proxy_call_finished(proxy, result, data=None):
            try:
                proxy.call_finish(result)
            except GLib.Error as e:
                print(
                    "Error: Failed to grab mediaplayer keys: {}".format(
                        e.message))

        self._media_keys_proxy.call(
            "GrabMediaPlayerKeys", GLib.Variant("(su)", ("Music", 0)),
            Gio.DBusCallFlags.NONE, -1, None, proxy_call_finished)

    def _handle_media_keys(self, proxy, sender, signal, parameters):
        app, response = parameters.unpack()
        if app != "Music":
            return

        if "Play" in response:
            self._player.play_pause()
        elif "Pause" in response:
            self._player.play_pause()
        elif "Stop" in response:
            self._player.stop()
        elif "Next" in response:
            self._player.next()
        elif "Previous" in response:
            self._player.prev()

