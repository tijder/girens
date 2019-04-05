#!/usr/bin/python

from http.server import HTTPServer, BaseHTTPRequestHandler
from socketserver import BaseServer
from urllib.parse import urlparse, parse_qs
from plexapi.server import PlexServer
from plexapi import BASE_HEADERS
from plexapi.playqueue import PlayQueue
from .player import Player

from io import BytesIO
import threading
import socket

class UdpListenner():
    def start(self):
        HOST = '0.0.0.0'
        PORT = 32412

        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.bind((HOST, PORT))
        while 1:
            data, address = sock.recvfrom(4096)
            if not data: break
            if (data == str.encode('M-SEARCH * HTTP/1.1\r\n\r\n')):
                answer = str.encode('HTTP/1.0 200 OK\r\nContent-Type: plex/media-player\r\nResource-Identifier: ' + BASE_HEADERS['X-Plex-Client-Identifier'] + '\r\nProduct: Girens\r\nPort: 32500\r\nProtocol-Version: 1\r\nUpdated-At: 1553552878\r\nProtocol-Capabilities: timeline,playback,playqueues\r\nVersion: 7.11.0.9072\r\nDevice-Class: mobile\r\nName: Giren Player\r\nProtocol: plex\r\n\r\n')
                print(address)
                print(answer)
                sock.sendto(answer, address)
        conn.close()


class SimpleHTTPRequestHandler(BaseHTTPRequestHandler):

    _resources = '''<?xml version="1.0" encoding="UTF-8"?>
<MediaContainer>
    <Player deviceClass="pc" machineIdentifier="''' + BASE_HEADERS['X-Plex-Client-Identifier'] + '''" platform="''' + BASE_HEADERS['X-Plex-Platform'] + '''" platformVersion="''' + BASE_HEADERS['X-Plex-Platform-Version'] + '''" product="Girens" protocolCapabilities="timeline,playback,playqueues" protocolVersion="1" title="''' + BASE_HEADERS['X-Plex-Device-Name'] + '''" version="''' + BASE_HEADERS['X-Plex-Version'] + '''"/>
</MediaContainer>'''

    _player = None

    def do_GET(self):
        if self._player is None:
            self._player = Player.getInstance()
        o = urlparse(self.path)
        print(o.path)
        q = parse_qs(o.query)
        print(q)
        self.send_response(200)
        self.end_headers()
        if 'commandID' in q:
            self._player.set_commandID(q['commandID'][0])
        if o.path == '/resources':
            self.wfile.write(str.encode(self._resources))
        elif o.path == '/player/timeline/subscribe':
            self._player.set_controller(q['protocol'][0] + '://' + self.client_address[0] + ':' + q['port'][0])
            self.wfile.write(bytes(self._player.get_timeline(), 'UTF-8'))
        elif o.path == '/player/timeline/unsubscribe':
            self._player.unset_controller()
            self.wfile.write(bytes("Failure: 200 OK", 'UTF-8'))
        elif o.path == '/player/playback/playMedia':
            thread = threading.Thread(target=self.handle_play_media, args=(q,))
            thread.daemon = True
            thread.start()
            self.wfile.write(bytes("Failure: 200 OK", 'UTF-8'))
        elif o.path == '/player/playback/pause':
            thread = threading.Thread(target=self.handle_playback_pause)
            thread.daemon = True
            thread.start()
            self.wfile.write(bytes("Failure: 200 OK", 'UTF-8'))
        elif o.path == '/player/playback/play':
            thread = threading.Thread(target=self.handle_playback_play)
            thread.daemon = True
            thread.start()
            self.wfile.write(bytes("Failure: 200 OK", 'UTF-8'))
        elif o.path == '/player/playback/stop':
            thread = threading.Thread(target=self.handle_playback_stop)
            thread.daemon = True
            thread.start()
            self.wfile.write(bytes("Failure: 200 OK", 'UTF-8'))

    def do_POST(self):
        # print(self.path)
        content_length = int(self.headers['Content-Length'])
        body = self.rfile.read(content_length)
        self.send_response(200)
        self.end_headers()
        response = BytesIO()
        response.write(b'This is POST request. ')
        response.write(b'Received: ')
        response.write(body)
        self.wfile.write(response.getvalue())

    def handle_play_media(self, q):
        print(q['token'][0])
        server = PlexServer('https://g4d.nl:32400', q['token'][0])
        playqueue = PlayQueue.get_from_url(server, q['containerKey'][0], q['key'][0])
        self._player.set_playqueue(playqueue)
        self._player.start()

    def handle_playback_pause(self):
        self._player.pause()

    def handle_playback_play(self):
        self._player.play()

    def handle_playback_stop(self):
        self._player.stop()


class ClientServer():
    def __init__(self, player):
        self._player = player

    def start(self):
        ul = UdpListenner()
        thread = threading.Thread(target=ul.start)
        thread.daemon = True
        thread.start()

        httpd = HTTPServer(('', 32500), SimpleHTTPRequestHandler)
        httpd.serve_forever()

