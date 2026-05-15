from pyln.client import NodeVersion
import pytest
import subprocess
import json
import threading
from aiohttp import web
from pyln.proto.bech32 import bech32_encode, convertbits
import pytest_asyncio
import asyncio


def get_cln_version():
    cln_version_proc = subprocess.check_output(["lightningd", "--version"])
    cln_version = NodeVersion(cln_version_proc.decode("ascii").strip())

    return cln_version


def pytest_configure(config):
    if not hasattr(config, "workerinput"):
        config.pay_renepay_deprecated = get_cln_version() >= NodeVersion("v26.06")


def pytest_configure_node(node):
    node.workerinput["pay_renepay_deprecated"] = node.config.pay_renepay_deprecated


@pytest.fixture(scope="session")
def pay_renepay_deprecated(request):
    if hasattr(request.config, "workerinput"):
        return request.config.workerinput["pay_renepay_deprecated"]

    return request.config.pay_renepay_deprecated


def encode_lnurl(url: str) -> str:
    data5 = convertbits(url.encode("utf-8"), 8, 5, True)
    data5 = bytes(data5)

    return bech32_encode("lnurl", data5).upper()


def run_app(app, host, port):
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)

    runner = web.AppRunner(app)

    async def _run():
        await runner.setup()
        site = web.TCPSite(runner, host, port)
        await site.start()
        while True:
            await asyncio.sleep(3600)

    loop.run_until_complete(_run())


@pytest_asyncio.fixture(scope="function")
async def lnurl_server(node_factory):
    node = node_factory.get_node(options={"log-level": "debug"})

    app = web.Application()

    HOST = "127.0.0.1"
    PORT = node_factory.get_unused_port()

    BASE = f"http://{HOST}:{PORT}"

    async def pay_params(request):
        callback = f"{BASE}/lnurl/callback"

        return web.json_response(
            {
                "callback": callback,
                "commentAllowed": 256,
                "minSendable": 1000,
                "maxSendable": 1_000_000,
                "metadata": json.dumps([["text/plain", "pytest lnurl server"]]),
                "tag": "payRequest",
            }
        )

    async def pay_callback(request):
        amount_msat = int(request.query["amount"])

        invoice_args = {
            "amount_msat": amount_msat,
            "label": f"test-{amount_msat}",
        }

        comment = request.query.get("comment")
        if comment is not None:
            invoice_args["description"] = request.query["comment"]
        else:
            invoice_args["description"] = "pytest lnurl server"

        inv = node.rpc.call("invoice", invoice_args)

        return web.json_response(
            {
                "pr": inv["bolt11"],
                "routes": [],
            }
        )

    app.router.add_get("/.well-known/lnurlp/test", pay_params)
    app.router.add_get("/lnurl/callback", pay_callback)

    thread = threading.Thread(
        target=run_app,
        args=(app, HOST, PORT),
        daemon=True,
    )
    thread.start()

    lnurl = encode_lnurl(f"{BASE}/.well-known/lnurlp/test")

    await asyncio.sleep(1)

    yield {"lnurl": lnurl, "node": node, "base": BASE}
