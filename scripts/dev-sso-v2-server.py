#!/usr/bin/env python3
"""Small AnyConnect SSO-v2 test gateway backed by a local ocserv instance.

The TLS listener implements the XML authentication exchange and proxies CSTP
CONNECT streams to ocserv. The HTTP listener represents an IdP: it encrypts an
alphanumeric token with the client's STRAP-DH public key and redirects the
system browser to OpenConnect's loopback callback.

This is development tooling, not a production SAML identity provider.
"""

from __future__ import annotations

import argparse
import base64
import html
import http.server
import os
import select
import socket
import ssl
import struct
import threading
import time
import urllib.parse
import xml.etree.ElementTree as ET

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.kdf.hkdf import HKDF


SSO_TOKEN_COOKIE = "sso-token"
SSO_ERROR_COOKIE = "sso-error"
SSO_CALLBACK = "http://localhost:29786/success"


def log(message: str) -> None:
    print(f"{time.strftime('%H:%M:%S')} {message}", flush=True)


class HttpStream:
    def __init__(self, sock: ssl.SSLSocket):
        self.sock = sock
        self.buffer = bytearray()

    def _fill(self, size: int) -> bool:
        while len(self.buffer) < size:
            chunk = self.sock.recv(65536)
            if not chunk:
                return False
            self.buffer.extend(chunk)
        return True

    def request(self) -> tuple[str, dict[str, str], bytes] | None:
        marker = b"\r\n\r\n"
        while marker not in self.buffer:
            chunk = self.sock.recv(65536)
            if not chunk:
                return None
            self.buffer.extend(chunk)
            if len(self.buffer) > 1024 * 1024:
                raise ValueError("HTTP headers are too large")

        header_end = self.buffer.index(marker)
        header_block = bytes(self.buffer[:header_end]).decode("iso-8859-1")
        del self.buffer[: header_end + len(marker)]
        lines = header_block.split("\r\n")
        headers: dict[str, str] = {}
        for line in lines[1:]:
            name, separator, value = line.partition(":")
            if separator:
                headers[name.strip().lower()] = value.strip()
        length = int(headers.get("content-length", "0"))
        if not self._fill(length):
            raise EOFError("truncated HTTP request body")
        body = bytes(self.buffer[:length])
        del self.buffer[:length]
        return lines[0], headers, body

    def response(self) -> tuple[str, dict[str, list[str]], bytes]:
        marker = b"\r\n\r\n"
        while marker not in self.buffer:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise EOFError("truncated HTTP response")
            self.buffer.extend(chunk)
        header_end = self.buffer.index(marker)
        header_block = bytes(self.buffer[:header_end]).decode("iso-8859-1")
        del self.buffer[: header_end + len(marker)]
        lines = header_block.split("\r\n")
        headers: dict[str, list[str]] = {}
        for line in lines[1:]:
            name, separator, value = line.partition(":")
            if separator:
                headers.setdefault(name.strip().lower(), []).append(value.strip())
        length = int(headers.get("content-length", ["0"])[-1])
        if not self._fill(length):
            raise EOFError("truncated HTTP response body")
        body = bytes(self.buffer[:length])
        del self.buffer[:length]
        return lines[0], headers, body


def http_response(body: str, status: str = "200 OK") -> bytes:
    encoded = body.encode()
    return (
        f"HTTP/1.1 {status}\r\n"
        "Content-Type: application/xml; charset=utf-8\r\n"
        f"Content-Length: {len(encoded)}\r\n"
        "Connection: keep-alive\r\n\r\n"
    ).encode() + encoded


def xml_request(auth_type: str, auth_values: dict[str, str] | None = None,
                opaque: ET.Element | None = None, device_id: str = "linux-64") -> bytes:
    root = ET.Element(
        "config-auth",
        {"client": "vpn", "type": auth_type, "aggregate-auth-version": "2"},
    )
    ET.SubElement(root, "version", {"who": "vpn"}).text = "h-openconnect-sso-test"
    ET.SubElement(root, "device-id").text = device_id
    ET.SubElement(root, "capabilities")
    if opaque is not None:
        root.append(opaque)
    if auth_values is not None:
        auth = ET.SubElement(root, "auth")
        for name, value in auth_values.items():
            ET.SubElement(auth, name).text = value
    if auth_type == "init":
        ET.SubElement(root, "group-access").text = "https://localhost/"
    return ET.tostring(root, encoding="utf-8", xml_declaration=True)


def post_xml(stream: HttpStream, host: str, path: str, body: bytes,
             cookie: str = "", user_agent: str = "h-openconnect-sso-test") -> tuple[dict[str, list[str]], bytes]:
    cookie_header = f"Cookie: {cookie}\r\n" if cookie else ""
    request = (
        f"POST {path} HTTP/1.1\r\n"
        f"Host: {host}\r\n"
        f"User-Agent: {user_agent}\r\n"
        "Content-Type: application/xml; charset=utf-8\r\n"
        "X-Aggregate-Auth: 1\r\n"
        f"Content-Length: {len(body)}\r\n"
        f"{cookie_header}"
        "Connection: keep-alive\r\n\r\n"
    ).encode() + body
    stream.sock.sendall(request)
    status, headers, response_body = stream.response()
    if " 200 " not in status:
        raise RuntimeError(f"ocserv authentication returned {status}")
    return headers, response_body


def update_cookies(cookies: dict[str, str], headers: dict[str, list[str]]) -> None:
    for value in headers.get("set-cookie", []):
        pair = value.split(";", 1)[0]
        name, separator, cookie_value = pair.partition("=")
        if separator:
            cookies[name] = cookie_value


def find_opaque(body: bytes) -> ET.Element | None:
    opaque = ET.fromstring(body).find("opaque")
    if opaque is None:
        return None
    return ET.fromstring(ET.tostring(opaque, encoding="utf-8"))


def authenticate_backend(args: argparse.Namespace) -> str:
    raw = socket.create_connection((args.backend_host, args.backend_port), timeout=15)
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    tls = context.wrap_socket(raw, server_hostname=args.backend_host)
    stream = HttpStream(tls)
    cookies: dict[str, str] = {}
    host = f"{args.backend_host}:{args.backend_port}"

    headers, body = post_xml(stream, host, "/", xml_request("init"))
    update_cookies(cookies, headers)
    opaque = find_opaque(body)
    cookie_header = "; ".join(f"{name}={value}" for name, value in cookies.items())
    headers, body = post_xml(
        stream,
        host,
        "/auth",
        xml_request("auth-reply", {"username": args.username}, opaque),
        cookie_header,
    )
    update_cookies(cookies, headers)
    opaque = find_opaque(body)
    cookie_header = "; ".join(f"{name}={value}" for name, value in cookies.items())
    headers, _ = post_xml(
        stream,
        host,
        "/auth",
        xml_request("auth-reply", {"password": args.password}, opaque),
        cookie_header,
    )
    update_cookies(cookies, headers)
    tls.close()
    token = cookies.get("webvpn", "")
    if not token:
        raise RuntimeError("ocserv did not issue a webvpn cookie")
    return token


def make_hpke_blob(public_key_b64: str, token: str) -> str:
    client_public = serialization.load_der_public_key(base64.b64decode(public_key_b64))
    if not isinstance(client_public, ec.EllipticCurvePublicKey):
        raise ValueError("STRAP-DH public key is not an EC key")
    server_private = ec.generate_private_key(ec.SECP256R1())
    shared_secret = server_private.exchange(ec.ECDH(), client_public)
    key = HKDF(
        algorithm=hashes.SHA256(), length=32, salt=None, info=b"AC_ECIES"
    ).derive(shared_secret)
    iv = os.urandom(12)
    encryptor = Cipher(algorithms.AES(key), modes.GCM(iv)).encryptor()
    ciphertext = encryptor.update(token.encode()) + encryptor.finalize()
    tag = encryptor.tag[:12]
    server_public = server_private.public_key().public_bytes(
        serialization.Encoding.DER,
        serialization.PublicFormat.SubjectPublicKeyInfo,
    )

    blob = bytearray(struct.pack(">H", 1))
    for kind, value in ((1, server_public), (2, tag), (3, ciphertext), (4, iv)):
        blob.extend(struct.pack(">HH", kind, len(value)))
        blob.extend(value)
    return base64.b64encode(blob).decode()


class SsoState:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.lock = threading.Lock()
        self.valid_frontend_tokens: dict[str, str] = {}

    def remember(self, token: str, device_id: str) -> None:
        with self.lock:
            self.valid_frontend_tokens[token] = device_id

    def device_id(self, token: str) -> str | None:
        with self.lock:
            return self.valid_frontend_tokens.get(token)


class IdpHandler(http.server.BaseHTTPRequestHandler):
    state: SsoState

    def do_GET(self) -> None:
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
        public_key = query.get("key", [""])[0]
        if not public_key:
            self.send_error(400, "missing STRAP-DH key")
            return
        token = f"HOPENSSO{int(time.time() * 1000)}"
        try:
            blob = make_hpke_blob(public_key, token)
        except Exception as error:
            log(f"IdP encryption failed: {error}")
            self.send_error(400, "invalid STRAP-DH key")
            return
        callback = (
            "http://localhost:29786/api/sso/"
            + urllib.parse.quote(blob, safe="")
            + "?return="
            + urllib.parse.quote(SSO_CALLBACK, safe="")
        )
        log("IdP redirecting system browser to the OpenConnect loopback callback")
        self.send_response(302)
        self.send_header("Location", callback)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, _format: str, *_args: object) -> None:
        return


def initial_sso_response(args: argparse.Namespace, public_key: str) -> str:
    login = (
        f"http://{args.browser_host}:{args.browser_port}/login?key="
        + urllib.parse.quote(public_key, safe="")
    )
    log(f"BROWSER_URL {login}")
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<config-auth client="vpn" type="auth-request" aggregate-auth-version="2">
  <auth id="sso">
    <banner>H-OpenConnect SSO-v2 E2E</banner>
    <sso-v2-login>{html.escape(login)}</sso-v2-login>
    <sso-v2-login-final>{SSO_CALLBACK}</sso-v2-login-final>
    <sso-v2-token-cookie-name>{SSO_TOKEN_COOKIE}</sso-v2-token-cookie-name>
    <sso-v2-error-cookie-name>{SSO_ERROR_COOKIE}</sso-v2-error-cookie-name>
    <sso-v2-browser-mode>external</sso-v2-browser-mode>
    <form method="post"><input type="sso" name="{SSO_TOKEN_COOKIE}" label="SSO"/></form>
  </auth>
</config-auth>'''


def complete_sso_response(backend_token: str) -> str:
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<config-auth client="vpn" type="complete" aggregate-auth-version="2">
  <session-token>{backend_token}</session-token>
  <auth id="success"/>
</config-auth>'''


def extract_webvpn_cookie(headers: dict[str, str]) -> str:
    for part in headers.get("cookie", "").split(";"):
        name, separator, value = part.strip().partition("=")
        if separator and name == "webvpn":
            return value
    return ""


def proxy_cstp(client: ssl.SSLSocket, first_line: str, headers: dict[str, str],
               state: SsoState) -> None:
    frontend_token = extract_webvpn_cookie(headers)
    device_id = state.device_id(frontend_token)
    if device_id is None:
        client.sendall(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n")
        raise RuntimeError("CSTP used an unknown SSO session token")

    args = state.args
    backend_token = authenticate_backend(args)
    raw = socket.create_connection((args.backend_host, args.backend_port), timeout=15)
    context = ssl.create_default_context()
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    backend = context.wrap_socket(raw, server_hostname=args.backend_host)
    # ocserv's CSTP parser is stricter than a general HTTP parser, including
    # canonical header spelling. Keep this internal hop deliberately minimal.
    forwarded = [
        "CONNECT /CSCOSSLC/tunnel HTTP/1.1",
        f"Host: {args.backend_host}:{args.backend_port}",
        "User-Agent: Open AnyConnect VPN Agent v9.20",
        f"Cookie: webvpn={backend_token}",
        "X-CSTP-Version: 1",
        "Connection: keep-alive",
        "",
        "",
    ]
    backend.sendall("\r\n".join(forwarded).encode("iso-8859-1"))
    backend_response = bytearray()
    while b"\r\n\r\n" not in backend_response:
        chunk = backend.recv(65536)
        if not chunk:
            raise EOFError("truncated ocserv CSTP response")
        backend_response.extend(chunk)
        if len(backend_response) > 1024 * 1024:
            raise ValueError("ocserv CSTP response headers are too large")
    status = bytes(backend_response).split(b"\r\n", 1)[0].decode("iso-8859-1")
    # Preserve ocserv's header spelling. OpenConnect's CSTP header parser is
    # intentionally protocol-specific rather than a general HTTP parser.
    client.sendall(backend_response)
    if " 200 " not in status:
        backend.close()
        raise RuntimeError(f"ocserv CSTP returned {status}")
    log("CSTP tunnel authenticated with the SSO-issued session and is now relaying")

    client.setblocking(False)
    backend.setblocking(False)
    sockets = (client, backend)
    try:
        while True:
            readable, _, exceptional = select.select(sockets, [], sockets, 30)
            if exceptional:
                return
            if not readable:
                continue
            for source in readable:
                destination = backend if source is client else client
                try:
                    data = source.recv(65536)
                except (ssl.SSLWantReadError, BlockingIOError):
                    continue
                if not data:
                    return
                destination.sendall(data)
    finally:
        backend.close()


def handle_gateway_client(raw: socket.socket, state: SsoState, tls_context: ssl.SSLContext) -> None:
    client: ssl.SSLSocket | None = None
    try:
        client = tls_context.wrap_socket(raw, server_side=True)
        stream = HttpStream(client)
        while True:
            request = stream.request()
            if request is None:
                return
            first_line, headers, body = request
            method = first_line.split(" ", 1)[0]
            if method == "CONNECT":
                proxy_cstp(client, first_line, headers, state)
                return
            if method != "POST":
                client.sendall(http_response("", "405 Method Not Allowed"))
                return
            root = ET.fromstring(body)
            auth_type = root.attrib.get("type", "")
            if auth_type == "init":
                public_key = headers.get("x-anyconnect-strap-dh-pubkey", "")
                if not public_key:
                    raise RuntimeError("client did not advertise STRAP-DH")
                client.sendall(http_response(initial_sso_response(state.args, public_key)))
                log("issued an external-browser SSO-v2 form")
            elif auth_type == "auth-reply":
                submitted = root.findtext(f"./auth/{SSO_TOKEN_COOKIE}", "")
                if not submitted.startswith("HOPENSSO") or not submitted.isalnum():
                    raise RuntimeError("client did not return the decrypted SSO token")
                device_id = root.findtext("device-id", "linux-64")
                frontend_token = f"HOPENSESSION{int(time.time() * 1000000)}"
                state.remember(frontend_token, device_id)
                client.sendall(http_response(complete_sso_response(frontend_token)))
                log("SSO-v2 callback accepted and ocserv session token issued")
            else:
                raise RuntimeError(f"unexpected XML auth type: {auth_type}")
    except Exception as error:
        log(f"gateway connection failed: {error}")
        if client is not None:
            try:
                client.sendall(http_response("", "500 Internal Server Error"))
            except Exception:
                pass
    finally:
        if client is not None:
            client.close()
        else:
            raw.close()


def serve_gateway(state: SsoState) -> None:
    args = state.args
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(args.cert, args.key)
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((args.bind, args.port))
    listener.listen(32)
    log(f"SSO-v2 TLS gateway listening on {args.bind}:{args.port}")
    while True:
        raw, _ = listener.accept()
        threading.Thread(
            target=handle_gateway_client,
            args=(raw, state, context),
            daemon=True,
        ).start()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bind", default="0.0.0.0")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--browser-host", default="10.0.2.2")
    parser.add_argument("--browser-port", type=int, required=True)
    parser.add_argument("--backend-host", default="127.0.0.1")
    parser.add_argument("--backend-port", type=int, required=True)
    parser.add_argument("--username", default="demo")
    parser.add_argument("--password", default="demo")
    parser.add_argument("--cert", required=True)
    parser.add_argument("--key", required=True)
    args = parser.parse_args()

    state = SsoState(args)
    IdpHandler.state = state
    httpd = http.server.ThreadingHTTPServer((args.bind, args.browser_port), IdpHandler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    log(f"test IdP listening on {args.bind}:{args.browser_port}")
    serve_gateway(state)


if __name__ == "__main__":
    main()
