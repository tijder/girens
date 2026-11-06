#!/usr/bin/env python3
"""Finish a sign-in to a development Ister without a browser.

    keycloak_sign_in.py <url file> [user] [password]

A debug build of Girens started with GIRENS_DEBUG_LOGIN_URL=<url file> writes
the page to sign in at to that file instead of opening it. This waits for the
file, signs in at the Keycloak of Ister's development stack (user and password
"ister") as a browser would, and ends at the port Girens listens on.
"""
import sys, time, os, re, html, urllib.request, urllib.parse, urllib.error
path = sys.argv[1]; user = sys.argv[2] if len(sys.argv) > 2 else "ister"; password = sys.argv[3] if len(sys.argv) > 3 else "ister"
for _ in range(120):
    if os.path.exists(path) and os.path.getsize(path): break
    time.sleep(0.5)
else:
    sys.exit("no sign-in page was written")
url = open(path).read().strip(); os.remove(path)
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k): return None
opener = urllib.request.build_opener(NoRedirect())
# The provider may send the browser back to Girens first, to be sent on to
# ask for less; follow until the form to sign in at shows.
for _ in range(5):
    try:
        first = opener.open(url)
        break
    except urllib.error.HTTPError as redirect:
        url = redirect.headers.get("Location")
        if not url: sys.exit(f"no sign-in page: {redirect.code}")
        print("sent on to", url.split("?")[0], "scope" in url and urllib.parse.parse_qs(urllib.parse.urlsplit(url).query).get("scope"))
else:
    sys.exit("sent around in circles")
# Keycloak marks its cookies Secure, which a cookie jar would hold back over http.
cookies = "; ".join(c.split(";")[0] for c in first.headers.get_all("Set-Cookie") or [])
page = first.read().decode()
action = html.unescape(re.search(r'<form[^>]*action="([^"]+)"', page).group(1))
data = urllib.parse.urlencode({"username": user, "password": password, "credentialId": ""}).encode()
try:
    answer = opener.open(urllib.request.Request(action, data, {"Cookie": cookies}))
    sys.exit(f"no redirect: {answer.status}")
except urllib.error.HTTPError as redirect:
    back = redirect.headers.get("Location")
    if not back: sys.exit(f"sign-in refused: {redirect.code}")
print("back to", back.split("?")[0])
print(urllib.request.urlopen(back).read().decode()[-70:])
