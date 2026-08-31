import pytest
import json
import threading
from aiohttp import web
from pyln.proto.bech32 import bech32_encode, convertbits
import pytest_asyncio
import asyncio


@pytest.fixture(scope="session", autouse=True)
def pay_renepay_deprecated():
    return True


@pytest.fixture(scope="session", autouse=True)
def xpay_payer_note_added():
    return True


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
    ADDRESS = f"{HOST}:{PORT}"

    async def pay_params(request):
        user = request.match_info["user"]
        callback = f"{BASE}/lnurl/callback?lnurlp={user}"

        return web.json_response(
            {
                "callback": callback,
                "commentAllowed": 256,
                "minSendable": 1000,
                "maxSendable": 1_000_000,
                "metadata": json.dumps(
                    [
                        ["text/plain", "pytest lnurl server"],
                        ["text/identifier", f"{user}@{ADDRESS}"],
                    ]
                ),
                "tag": "payRequest",
            }
        )

    async def pay_callback(request):
        amount_msat = int(request.query["amount"])
        user = request.query["lnurlp"]

        invoice_args = {
            "amount_msat": amount_msat,
            "label": f"{user}-{amount_msat}",
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

    app.router.add_get("/.well-known/lnurlp/{user}", pay_params)
    app.router.add_get("/lnurl/callback", pay_callback)

    thread = threading.Thread(
        target=run_app,
        args=(app, HOST, PORT),
        daemon=True,
    )
    thread.start()

    lnurl = encode_lnurl(f"{BASE}/.well-known/lnurlp/test")

    await asyncio.sleep(1)

    yield {"lnurl": lnurl, "node": node, "base": BASE, "address": ADDRESS}
