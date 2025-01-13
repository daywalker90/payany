#!/usr/bin/env python3

import logging

import pytest
from pyln.client import RpcError
from pyln.testing.fixtures import *  # noqa: F403
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
    offer = l2.rpc.call("offer", {"amount": "any"})
    result = l1.rpc.call(
        "payany",
        {
            "invstring": offer["bolt12"],
            "amount_msat": 1_000,
            "message": "test1",
        },
    )
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
    decoded = l1.rpc.call("decode", {"string": result["invoice"]})
    assert decoded["invoice_amount_msat"] == 2_000
    assert decoded["invreq_payer_note"] == "test2"


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
    offer = l2.rpc.call("offer", {"amount": "any"})
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
    offer = l2.rpc.call("offer", {"amount": "any"})
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
