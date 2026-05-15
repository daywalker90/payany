#!/usr/bin/env python3

import json
import logging
import os
import time

import pytest
from pathlib import Path
from pyln.client import RpcError
from pyln.testing.fixtures import *  # noqa: F403
from pyln.testing.utils import wait_for
from util import get_plugin  # noqa: F401

LOGGER = logging.getLogger(__name__)


def test_payany_with_offer(node_factory, get_plugin):  # noqa: F811
    opts = [{"plugin": get_plugin, "log-level": "debug"}, {"log-level": "debug"}]

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )

    offer = l2.rpc.call("offer", {"amount": "any", "description": "testpayany"})
    with pytest.raises(
        RpcError, match="offer has `any` amount, must specify `amount_msat`"
    ):
        l1.rpc.call(
            "payany",
            {
                "invstring": offer["bolt12"],
            },
        )
    time.sleep(1)
    result = l1.rpc.call(
        "payany",
        {
            "invstring": offer["bolt12"],
            "amount_msat": 1_000,
            "message": "test1",
        },
    )
    time.sleep(1)
    decoded = l1.rpc.call("decode", {"string": result["invoice"]})
    assert decoded["invoice_amount_msat"] == 1_000
    assert decoded["invreq_payer_note"] == "test1"

    offer = l2.rpc.call("offer", {"amount": 2_000, "description": "testing"})
    result = l1.rpc.call(
        "payany",
        {
            "invstring": offer["bolt12"],
            "amount_msat": 2_000,
            "message": "test2",
        },
    )
    time.sleep(1)
    decoded = l1.rpc.call("decode", {"string": result["invoice"]})
    assert decoded["invoice_amount_msat"] == 2_000
    assert decoded["invreq_payer_note"] == "test2"

    result = l1.rpc.call(
        "payany",
        {
            "invstring": offer["bolt12"],
            "message": "test3",
        },
    )
    decoded = l1.rpc.call("decode", {"string": result["invoice"]})
    assert decoded["invoice_amount_msat"] == 2_000
    assert decoded["invreq_payer_note"] == "test3"


def test_xpay_supercharged(node_factory, get_plugin):  # noqa: F811
    opts = [{"plugin": get_plugin, "log-level": "debug"}, {"log-level": "debug"}]

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )
    offer = l2.rpc.call("offer", {"amount": "any", "description": "testpayany"})
    result = l1.rpc.call(
        "xpay", {"invstring": offer["bolt12"], "amount_msat": 3_000, "message": "test3"}
    )
    assert result["amount_msat"] == 3_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[0]["amount_msat"] == 3_000
    assert pay[0]["invreq_payer_note"] == "test3"

    offer = l2.rpc.call("offer", {"amount": 2_000, "description": "testing"})
    result = l1.rpc.call("xpay", [offer["bolt12"], 2_000])
    assert result["amount_msat"] == 2_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[1]["amount_msat"] == 2_000
    assert "invreq_payer_note" not in pay[1]

    with pytest.raises(
        RpcError, match="missing required parameter: `invstring`/`bolt11`"
    ):
        result = l1.rpc.call("xpay", [])


def test_pay_supercharged(node_factory, get_plugin, pay_renepay_deprecated):  # noqa: F811
    opts = [
        {"plugin": get_plugin, "log-level": "debug"},
        {"log-level": "debug"},
    ]
    if pay_renepay_deprecated:
        opts[0]["allow-deprecated-apis"] = True

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )
    offer = l2.rpc.call("offer", {"amount": "any", "description": "testpayany"})
    result = l1.rpc.call(
        "pay", {"bolt11": offer["bolt12"], "amount_msat": 3_000, "message": "test3"}
    )
    assert result["amount_msat"] == 3_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[0]["amount_msat"] == 3_000
    assert pay[0]["invreq_payer_note"] == "test3"

    offer = l2.rpc.call("offer", {"amount": 2_000, "description": "testing"})
    result = l1.rpc.call("pay", [offer["bolt12"], 2_000])
    assert result["amount_msat"] == 2_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[1]["amount_msat"] == 2_000
    assert "invreq_payer_note" not in pay[1]

    with pytest.raises(
        RpcError, match="missing required parameter: `invstring`/`bolt11`"
    ):
        result = l1.rpc.call("pay", [])


def test_renepay_supercharged(node_factory, get_plugin, pay_renepay_deprecated):  # noqa: F811
    opts = [{"plugin": get_plugin, "log-level": "debug"}, {"log-level": "debug"}]
    if pay_renepay_deprecated:
        opts[0]["allow-deprecated-apis"] = True

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )
    offer = l2.rpc.call("offer", {"amount": "any", "description": "testpayany"})
    result = l1.rpc.call(
        "renepay",
        {"invstring": offer["bolt12"], "amount_msat": 3_000, "message": "test3"},
    )
    assert result["amount_msat"] == 3_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[0]["amount_msat"] == 3_000
    assert pay[0]["invreq_payer_note"] == "test3"

    offer = l2.rpc.call("offer", {"amount": 2_000, "description": "testing"})
    result = l1.rpc.call("renepay", [offer["bolt12"], 2_000])
    assert result["amount_msat"] == 2_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[1]["amount_msat"] == 2_000
    assert "invreq_payer_note" not in pay[1]

    with pytest.raises(
        RpcError, match="missing required parameter: `invstring`/`bolt11`"
    ):
        result = l1.rpc.call("renepay", [])


def test_budget(node_factory, get_plugin, pay_renepay_deprecated):  # noqa: F811
    opts = [
        {
            "plugin": get_plugin,
            "payany-budget-per": "5 hours",
            "payany-budget-amount-msat": 1000000,
            "payany-xpay-handle-pay": True,
            "log-level": "debug",
        },
        {"log-level": "debug", "fee-base": 1000, "fee-per-satoshi": 10},
        {"log-level": "debug"},
    ]

    l1, l2, l3 = node_factory.line_graph(
        3,
        wait_for_announce=True,
        opts=opts,
    )
    l1.daemon.logsearch_start = 0
    l1.daemon.wait_for_log("Budget set to 1000000msat every 18000seconds")

    config = l2.rpc.call("listconfigs")["configs"]
    assert config["fee-base"]["value_int"] == 1000
    assert config["fee-per-satoshi"]["value_int"] == 10

    invoice1 = l3.rpc.call("invoice", [950000, "test", "test"])
    l1.rpc.call("xpay", invoice1["bolt11"])

    pays = l1.rpc.call("listpays")["pays"][0]["amount_sent_msat"]
    assert pays == 951009

    invoice2 = l3.rpc.call("invoice", [950000, "test2", "test2"])
    with pytest.raises(
        RpcError,
        match="payany budget exceeded: Budget would be exceeded! 1910509msat / 1000000msat",
    ):
        l1.rpc.call("xpay", invoice2["bolt11"])

    l1.rpc.call("setconfig", ["payany-budget-amount-msat", 2000000])

    xpay_params = {
        "maxfee": 5000,
        "retry_for": 30,
        "maxdelay": 200,
        "payer_note": "note",
    }
    if pay_renepay_deprecated:
        xpay_params["label"] = "ignored"

    invoice3 = l3.rpc.call("invoice", [950000, "test3", "test3"])
    l1.rpc.call(
        "xpay",
        {"invstring": invoice3["bolt11"], **xpay_params},
    )

    invoice4 = l3.rpc.call("invoice", [950000, "test4", "test4"])
    with pytest.raises(
        RpcError,
        match="payany budget exceeded: Budget would be exceeded! 2857018msat / 2000000msat",
    ):
        l1.rpc.call(
            "xpay",
            {"invstring": invoice4["bolt11"], **xpay_params},
        )

    l1.rpc.call("setconfig", ["payany-budget-amount-msat", 3000000])

    invoice5 = l3.rpc.call("invoice", [950000, "test5", "test5"])
    l1.rpc.call("askrene-create-layer", ["testbudget"])
    l1.rpc.call("askrene-disable-node", ["testbudget", l2.info["id"]])
    with pytest.raises(RpcError, match="We could not find a usable set of paths"):
        l1.rpc.call(
            "xpay",
            {"invstring": invoice5["bolt11"], "layers": ["testbudget"], **xpay_params},
        )

    c3 = l3.rpc.call("listpeerchannels")["channels"][0]["short_channel_id"]
    l1.rpc.call("askrene-create-layer", ["testbudget2"])
    l1.rpc.call("askrene-update-channel", ["testbudget2", c3 + "/0", False])
    l1.rpc.call("askrene-update-channel", ["testbudget2", c3 + "/1", False])
    with pytest.raises(RpcError, match="We could not find a usable set of paths"):
        l1.rpc.call(
            "xpay",
            {"invstring": invoice5["bolt11"], "layers": ["testbudget2"], **xpay_params},
        )

    invoice6 = l3.rpc.call(
        "invoice",
        {
            "amount_msat": 950000,
            "label": "test6",
            "description": "test6",
            "deschashonly": True,
        },
    )
    l1.rpc.call(
        "xpay",
        {"invstring": invoice6["bolt11"], **xpay_params},
    )

    if not pay_renepay_deprecated:
        return

    l1.rpc.call("setconfig", ["payany-budget-amount-msat", 4000000])
    offer = l3.rpc.call("offer", {"amount": 950000, "description": "testpayany"})
    bolt12 = l1.rpc.call("fetchinvoice", [offer["bolt12"]])
    with pytest.raises(
        RpcError,
        match="Unknown invoice_request 7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
    ):
        l1.rpc.call(
            "xpay",
            {
                "invstring": bolt12["invoice"],
                "label": "ignored",
                "maxfee": 3000,
                "retry_for": 30,
                "maxdelay": 200,
                "payer_note": "test3",
                "localinvreqid": "7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
            },
        )


def test_handle_opt(node_factory, get_plugin):  # noqa: F811
    opts = {
        "xpay-handle-pay": True,
        "log-level": "debug",
    }

    l1 = node_factory.get_node(
        options=opts,
    )

    l1.rpc.call("plugin", {"subcommand": "start", "plugin": str(get_plugin)})
    l1.daemon.wait_for_log(
        "Found activated `xpay-handle-pay`, `payany` deactivated it!"
    )

    conf = l1.rpc.call("listconfigs", {"config": "xpay-handle-pay"})
    assert conf["configs"]["xpay-handle-pay"]["value_bool"] is False

    with pytest.raises(
        RpcError,
        match="Setting xpay-handle-pay to true when payany is active is blocked",
    ):
        l1.rpc.call("setconfig", {"config": "xpay-handle-pay"})

    conf = l1.rpc.call("listconfigs", {"config": "xpay-handle-pay"})
    assert conf["configs"]["xpay-handle-pay"]["value_bool"] is False

    with pytest.raises(
        RpcError,
        match="Setting xpay-handle-pay to true when payany is active is blocked",
    ):
        l1.rpc.call("setconfig", {"config": "xpay-handle-pay", "val": True})

    conf = l1.rpc.call("listconfigs", {"config": "xpay-handle-pay"})
    assert conf["configs"]["xpay-handle-pay"]["value_bool"] is False

    with pytest.raises(
        RpcError,
        match="Setting xpay-handle-pay to true when payany is active is blocked",
    ):
        l1.rpc.call("setconfig", ["xpay-handle-pay", True])

    conf = l1.rpc.call("listconfigs", {"config": "xpay-handle-pay"})
    assert conf["configs"]["xpay-handle-pay"]["value_bool"] is False

    with pytest.raises(
        RpcError,
        match="Setting xpay-handle-pay to true when payany is active is blocked",
    ):
        l1.rpc.call("setconfig", "xpay-handle-pay")

    conf = l1.rpc.call("listconfigs", {"config": "xpay-handle-pay"})
    assert conf["configs"]["xpay-handle-pay"]["value_bool"] is False

    l1.rpc.call("setconfig", {"config": "xpay-handle-pay", "val": False})
    conf = l1.rpc.call("listconfigs", {"config": "xpay-handle-pay"})
    assert conf["configs"]["xpay-handle-pay"]["value_bool"] is False


def test_pay_to_xpay_fees(node_factory, get_plugin, pay_renepay_deprecated):  # noqa: F811
    opts = [
        {
            "plugin": get_plugin,
            "payany-xpay-handle-pay": True,
            "log-level": "debug",
        },
        {"log-level": "debug"},
        {"log-level": "debug"},
    ]
    if pay_renepay_deprecated:
        opts[0]["allow-deprecated-apis"] = True

    l1, l2, l3 = node_factory.line_graph(
        3,
        wait_for_announce=True,
        opts=opts,
    )

    ch1 = l2.rpc.call("listpeerchannels", {"id": l3.info["id"]})["channels"][0][
        "short_channel_id"
    ]
    l2.rpc.call("setchannel", {"id": ch1, "feebase": 10000, "enforcedelay": 0})

    wait_for(
        lambda: (
            l1.rpc.call("listchannels", {"destination": l3.info["id"]})["channels"][0][
                "base_fee_millisatoshi"
            ]
            == 10000
        )
    )
    wait_for(
        lambda: (
            l3.rpc.call("listchannels", {"destination": l3.info["id"]})["channels"][0][
                "base_fee_millisatoshi"
            ]
            == 10000
        )
    )

    invoice = l3.rpc.call("invoice", [950000, "test", "test"])

    with pytest.raises(
        RpcError,
        match="Could not find route without excessive cost",
    ):
        l1.rpc.call("pay", {"bolt11": invoice["bolt11"]})

    l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "maxfeepercent": 1.1})

    invoice = l3.rpc.call("invoice", [950000, "test2", "test2"])
    l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "maxfee": 10010})

    invoice = l3.rpc.call("invoice", [950000, "test3", "test3"])
    l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "exemptfee": 10010})

    invoice = l3.rpc.call("invoice", [950000, "test4", "test4"])
    l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "maxfeepercent": "1.1"})

    invoice = l3.rpc.call("invoice", [950000, "test5", "test5"])
    with pytest.raises(
        RpcError,
        match="maxfeepercent: cound not parse string as a floating-point number: 5%",
    ):
        l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "maxfeepercent": "5%"})

    with pytest.raises(
        RpcError,
        match="maxfeepercent is not a number or string!",
    ):
        l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "maxfeepercent": [1.1]})

    with pytest.raises(
        RpcError,
        match="maxfeepercent must be positive!",
    ):
        l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "maxfeepercent": -1.0})


def test_lnurl(node_factory, get_plugin):  # noqa: F811
    port = node_factory.get_unused_port()
    url = f"127.0.0.1:{port}"
    user_name = "testuser"
    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=[
            {
                "log-level": "debug",
                "plugin": get_plugin,
                "payany-strict-lnurl": True,
            },
            {
                "log-level": "debug",
                "plugin": os.path.join(Path(__file__).parent.resolve(), "clnaddress"),
                "clnaddress-listen": url,
                "clnaddress-base-url": f"http://{url}/",
                "clnaddress-description": "testing_description",
            },
        ],
    )
    wait_for(lambda: l2.daemon.is_in_log("Starting lnurlp server."))

    l2.rpc.call("clnaddress-adduser", [user_name])

    l1.rpc.call("xpay", {"invstring": f"{user_name}@{url}", "amount_msat": 2500})
    invoice = l2.rpc.call("listinvoices", {})["invoices"][0]
    assert invoice["status"] == "paid"
    assert invoice["amount_received_msat"] == 2500
    assert json.loads(invoice["description"]) == [
        ["text/plain", "testing_description"],
        ["text/identifier", f"testuser@{url}"],
    ]

    l2.rpc.call("clnaddress-adduser", [user_name, True, "testing_description2"])

    l1.rpc.call("xpay", {"invstring": f"{user_name}@{url}", "amount_msat": 2600})
    invoice = l2.rpc.call("listinvoices", {})["invoices"][1]
    assert invoice["status"] == "paid"
    assert invoice["amount_received_msat"] == 2600
    assert json.loads(invoice["description"]) == [
        ["text/plain", "testing_description2"],
        ["text/email", f"testuser@{url}"],
    ]

    with pytest.raises(RpcError, match="404"):
        l1.rpc.call("xpay", {"invstring": f"fakeuser@{url}", "amount_msat": 2600})
