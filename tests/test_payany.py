#!/usr/bin/env python3

import json
import logging
import os
import time

import pytest
from pyln.client import RpcError
from pyln.testing.fixtures import *  # noqa: F403
from pyln.testing.utils import wait_for
from util import experimental_offers_check, get_plugin  # noqa: F401

LOGGER = logging.getLogger(__name__)


def test_payany_with_offer(node_factory, get_plugin):  # noqa: F811
    opts = [{"plugin": get_plugin, "log-level": "debug"}, {"log-level": "debug"}]
    if experimental_offers_check(node_factory):
        opts[0]["experimental-offers"] = None
        opts[1]["experimental-offers"] = None

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
    if experimental_offers_check(node_factory):
        opts[0]["experimental-offers"] = None
        opts[1]["experimental-offers"] = None

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )
    version = l1.rpc.getinfo()["version"]
    if version.startswith("v24.0"):
        return
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

    with pytest.raises(RpcError, match="missing required parameter: invstring"):
        result = l1.rpc.call("xpay", [])


def test_pay_supercharged(node_factory, get_plugin):  # noqa: F811
    opts = [{"plugin": get_plugin, "log-level": "debug"}, {"log-level": "debug"}]
    if experimental_offers_check(node_factory):
        opts[0]["experimental-offers"] = None
        opts[1]["experimental-offers"] = None

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

    with pytest.raises(RpcError, match="missing required parameter: bolt11"):
        result = l1.rpc.call("pay", [])


def test_renepay_supercharged(node_factory, get_plugin):  # noqa: F811
    opts = [{"plugin": get_plugin, "log-level": "debug"}, {"log-level": "debug"}]
    if experimental_offers_check(node_factory):
        opts[0]["experimental-offers"] = None
        opts[1]["experimental-offers"] = None

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )
    version = l1.rpc.getinfo()["version"]
    if version.startswith("v24.") or version.startswith("v23."):
        # these cln versions don't support bolt12 invoices with renepay
        return
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

    with pytest.raises(RpcError, match="missing required parameter: invstring"):
        result = l1.rpc.call("renepay", [])


def test_budget(node_factory, get_plugin):  # noqa: F811
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
    version = l1.rpc.getinfo()["version"]
    if version.startswith("v24.0") or version.startswith("v23."):
        # old cln versions pay command is not finding routes this tight
        return
    l1.daemon.logsearch_start = 0
    l1.daemon.wait_for_log("Budget set to 1000000msat every 18000seconds")

    config = l2.rpc.call("listconfigs")["configs"]
    assert config["fee-base"]["value_int"] == 1000
    assert config["fee-per-satoshi"]["value_int"] == 10

    invoice = l3.rpc.call("invoice", [950000, "test", "test"])
    l1.rpc.call("pay", invoice["bolt11"])

    pays = l1.rpc.call("listpays")["pays"][0]["amount_sent_msat"]
    assert pays == 951009

    invoice = l3.rpc.call("invoice", [950000, "test2", "test2"])
    with pytest.raises(
        RpcError,
        match="payany budget exceeded: Budget would be exceeded! 1910509msat / 1000000msat",
    ):
        l1.rpc.call("pay", invoice["bolt11"])

    l1.rpc.call("setconfig", ["payany-budget-amount-msat", 2000000])

    invoice = l3.rpc.call("invoice", [950000, "test3", "test3"])
    l1.rpc.call(
        "pay",
        {
            "bolt11": invoice["bolt11"],
            "label": "ignored",
            "riskfactor": 5,
            "maxfeepercent": 2,
            "retry_for": 30,
            "maxdelay": 200,
            "exemptfee": 6000,
            "localinvreqid": "7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
            "description": "test3",
        },
    )

    invoice = l3.rpc.call("invoice", [950000, "test4", "test4"])
    with pytest.raises(
        RpcError,
        match="payany budget exceeded: Budget would be exceeded! 2871018msat / 2000000msat",
    ):
        l1.rpc.call(
            "pay",
            {
                "bolt11": invoice["bolt11"],
                "label": "ignored",
                "riskfactor": 5,
                "maxfeepercent": 2,
                "retry_for": 30,
                "maxdelay": 200,
                "exemptfee": 6000,
                "localinvreqid": "7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
                "description": "test3",
            },
        )

    l1.rpc.call("setconfig", ["payany-budget-amount-msat", 3000000])

    invoice = l3.rpc.call("invoice", [950000, "test5", "test5"])
    with pytest.raises(RpcError, match="We could not find a usable set of paths"):
        l1.rpc.call(
            "pay",
            {
                "bolt11": invoice["bolt11"],
                "label": "ignored",
                "riskfactor": 5,
                "maxfeepercent": 2,
                "retry_for": 30,
                "maxdelay": 200,
                "exemptfee": 6000,
                "localinvreqid": "7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
                "description": "test3",
                "exclude": [l2.info["id"]],
            },
        )

    c3 = l3.rpc.call("listpeerchannels")["channels"][0]["short_channel_id"]
    with pytest.raises(RpcError, match="We could not find a usable set of paths"):
        l1.rpc.call(
            "pay",
            {
                "bolt11": invoice["bolt11"],
                "label": "ignored",
                "riskfactor": 5,
                "maxfeepercent": 2,
                "retry_for": 30,
                "maxdelay": 200,
                "exemptfee": 6000,
                "localinvreqid": "7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
                "description": "test3",
                "exclude": [c3 + "/0", c3 + "/1"],
            },
        )

    invoice = l3.rpc.call(
        "invoice",
        {
            "amount_msat": 950000,
            "label": "test6",
            "description": "test6",
            "deschashonly": True,
        },
    )
    l1.rpc.call(
        "pay",
        {
            "bolt11": invoice["bolt11"],
            "label": "ignored",
            "riskfactor": 5,
            "maxfee": 3000,
            "retry_for": 30,
            "maxdelay": 200,
            "localinvreqid": "7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
            "description": "test3",
        },
    )


def test_handle_opt(node_factory, get_plugin):  # noqa: F811
    lx = node_factory.get_node()
    version = lx.rpc.getinfo()["version"]
    if version.startswith("v24.0"):
        return

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


def test_pay_to_xpay_fees(node_factory, get_plugin):  # noqa: F811
    opts = [
        {
            "plugin": get_plugin,
            "payany-xpay-handle-pay": True,
            "log-level": "debug",
        },
        {"log-level": "debug"},
        {"log-level": "debug"},
    ]

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
        lambda: l1.rpc.call("listchannels", {"destination": l3.info["id"]})["channels"][
            0
        ]["base_fee_millisatoshi"]
        == 10000
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
                "plugin": os.path.join(os.getcwd(), "tests/clnaddress"),
                "clnaddress-listen": url,
                "clnaddress-base-url": f"http://{url}/",
                "clnaddress-description": "testing_description",
            },
        ],
    )
    wait_for(lambda: l2.daemon.is_in_log("Starting lnurlp server."))

    l2.rpc.call("clnaddress-adduser", [user_name])

    pay = l1.rpc.call("pay", {"bolt11": f"{user_name}@{url}", "amount_msat": 2500})
    invoice = l2.rpc.call("listinvoices", {"payment_hash": pay["payment_hash"]})[
        "invoices"
    ][0]
    assert invoice["status"] == "paid"
    assert invoice["amount_received_msat"] == 2500
    assert json.loads(invoice["description"]) == [
        ["text/plain", "testing_description"],
        ["text/identifier", f"testuser@{url}"],
    ]

    l2.rpc.call("clnaddress-adduser", [user_name, True, "testing_description2"])

    pay = l1.rpc.call("pay", {"bolt11": f"{user_name}@{url}", "amount_msat": 2600})
    invoice = l2.rpc.call("listinvoices", {"payment_hash": pay["payment_hash"]})[
        "invoices"
    ][0]
    assert invoice["status"] == "paid"
    assert invoice["amount_received_msat"] == 2600
    assert json.loads(invoice["description"]) == [
        ["text/plain", "testing_description2"],
        ["text/email", f"testuser@{url}"],
    ]

    with pytest.raises(RpcError, match="404"):
        pay = l1.rpc.call("pay", {"bolt11": f"fakeuser@{url}", "amount_msat": 2600})
