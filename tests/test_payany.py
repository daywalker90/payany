import json
import logging
import os
import threading
from pathlib import Path

import pytest
from pyln.client import RpcError
from pyln.testing.fixtures import *
from pyln.testing.utils import wait_for
from util import get_plugin  # noqa: F401

LOGGER = logging.getLogger(__name__)


def test_payany_with_offer_and_bi353(
    node_factory,
    lnurl_server,
    pay_renepay_deprecated,
    get_plugin,  # noqa: F811
):
    opts = {
        "plugin": get_plugin,
        "payany-budget-per": "5 hours",
        "payany-budget-amount-msat": 1000000,
        "log-level": "debug",
    }
    if pay_renepay_deprecated:
        opts["allow-deprecated-apis"] = True

    l1 = node_factory.get_node(
        options=opts,
    )
    l2 = lnurl_server["node"]
    l1.fundchannel(l2, 1_000_000, wait_for_active=True)

    address = f"payme@{lnurl_server['address']}"

    offer = l2.rpc.call("offer", {"amount": "any", "description": "testpayany"})
    result = l1.rpc.call(
        "payany",
        {
            "invstring": offer["bolt12"],
            "amount_msat": 1_000,
            "message": "test1",
        },
    )
    assert result["invoice"] == offer["bolt12"]

    result = l1.rpc.call(
        "payany",
        {
            "invstring": address,
            "amount_msat": 2_000,
            "message": "test2",
        },
    )

    l1.rpc.call("xpay", [offer["bolt12"], 3_000])

    l1.rpc.call("pay", [f"payme@{lnurl_server['address']}", 1_001])


def test_xpay_supercharged(node_factory, get_plugin, lnurl_server):  # noqa: F811
    opts = {"plugin": get_plugin, "log-level": "debug"}

    l1 = node_factory.get_node(
        options=opts,
    )
    l2 = lnurl_server["node"]
    l1.fundchannel(l2, 1_000_000, wait_for_active=True)

    lnurl = lnurl_server["lnurl"]
    result = l1.rpc.call(
        "xpay", {"invstring": lnurl, "amount_msat": 3_000, "message": "test3"}
    )
    assert result["amount_msat"] == 3_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[0]["amount_msat"] == 3_000
    assert pay[0]["description"] == "test3"

    result = l1.rpc.call("xpay", [lnurl, 2_000])
    assert result["amount_msat"] == 2_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[1]["amount_msat"] == 2_000
    assert pay[1]["description"] == "pytest lnurl server"

    with pytest.raises(RpcError, match="missing required parameter"):
        result = l1.rpc.call("xpay", [])


def test_pay_supercharged(
    node_factory,
    get_plugin,  # noqa: F811
    pay_renepay_deprecated,
    lnurl_server,
):
    opts = {"plugin": get_plugin, "log-level": "debug"}
    if pay_renepay_deprecated:
        opts["allow-deprecated-apis"] = True

    l1 = node_factory.get_node(
        options=opts,
    )
    l2 = lnurl_server["node"]
    l1.fundchannel(l2, 1_000_000, wait_for_active=True)

    lnurl = lnurl_server["lnurl"]
    result = l1.rpc.call(
        "pay", {"bolt11": lnurl, "amount_msat": 3_000, "message": "test3"}
    )
    assert result["amount_msat"] == 3_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[0]["amount_msat"] == 3_000
    assert pay[0]["description"] == "test3"

    result = l1.rpc.call("pay", [lnurl, 2_000])
    assert result["amount_msat"] == 2_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[1]["amount_msat"] == 2_000
    assert pay[1]["description"] == "pytest lnurl server"

    with pytest.raises(RpcError, match="missing required parameter"):
        result = l1.rpc.call("pay", [])

    l1.rpc.call("plugin", ["stop", "payany"])
    l1.rpc.call(
        "plugin",
        {
            "subcommand": "start",
            "plugin": str(get_plugin),
            "payany-budget-per": "5 hours",
            "payany-budget-amount-msat": 1000000,
        },
    )

    result = l1.rpc.call(
        "pay", {"bolt11": lnurl, "amount_msat": 3_001, "message": "test4"}
    )
    assert result["amount_msat"] == 3_001
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[2]["amount_msat"] == 3_001
    assert pay[2]["description"] == "test4"


@pytest.mark.asyncio
async def test_renepay_supercharged(
    node_factory,
    get_plugin,  # noqa: F811
    pay_renepay_deprecated,
    lnurl_server,
):
    opts = {"plugin": get_plugin, "log-level": "debug"}
    if pay_renepay_deprecated:
        opts["allow-deprecated-apis"] = True

    l1 = node_factory.get_node(
        options=opts,
    )
    l2 = lnurl_server["node"]
    l1.fundchannel(l2, 1_000_000, wait_for_active=True)

    lnurl = lnurl_server["lnurl"]
    result = l1.rpc.call(
        "renepay",
        {"invstring": lnurl, "amount_msat": 3_000, "message": "test3"},
    )
    assert result["amount_msat"] == 3_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[0]["amount_msat"] == 3_000
    assert pay[0]["description"] == "test3"

    result = l1.rpc.call("renepay", [lnurl, 2_000])
    assert result["amount_msat"] == 2_000
    pay = l2.rpc.call("listinvoices", {})["invoices"]
    assert pay[1]["amount_msat"] == 2_000
    assert pay[1]["description"] == "pytest lnurl server"

    with pytest.raises(RpcError, match="missing required parameter"):
        result = l1.rpc.call("renepay", [])


def test_budget(
    node_factory,
    get_plugin,  # noqa: F811
    pay_renepay_deprecated,
    xpay_payer_note_added,
):
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
    }
    if xpay_payer_note_added:
        xpay_params["payer_note"] = "note"
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

    l1.rpc.call("setconfig", ["payany-budget-amount-msat", 4000000])

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

    l1.rpc.call("setconfig", ["payany-budget-amount-msat", 5000000])
    offer = l3.rpc.call("offer", {"amount": 950000, "description": "testpayany"})
    bolt12 = l1.rpc.call("fetchinvoice", [offer["bolt12"]])
    xpay_params = {
        "invstring": bolt12["invoice"],
        "label": "ignored",
        "maxfee": 3000,
        "retry_for": 30,
        "maxdelay": 200,
        "localinvreqid": "7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
    }
    if xpay_payer_note_added:
        xpay_params["payer_note"] = "test3"
    with pytest.raises(
        RpcError,
        match="Unknown invoice_request 7f9b2c6d7a9b3b204b6d3cfe8d88f9b42b650cd6c57df3a4e1f7a08d14968e2c",
    ):
        l1.rpc.call(
            "xpay",
            xpay_params,
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

    wait_for(
        lambda: (
            not l1.rpc.call("listconfigs", {"config": "xpay-handle-pay"})["configs"][
                "xpay-handle-pay"
            ]["value_bool"]
        )
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

    with pytest.raises(RpcError, match="invalid address"):
        l1.rpc.call("xpay", {"invstring": f"fakeuser@{url}", "amount_msat": 2600})


def test_budget_concurrent(node_factory, get_plugin):  # noqa: F811
    opts = [
        {
            "plugin": get_plugin,
            "payany-budget-per": "5 hours",
            "payany-budget-amount-msat": 1000000,
            "log-level": "debug",
        },
        {"log-level": "debug"},
    ]

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )

    invoice1 = l2.rpc.call("invoice", [600000, "concurrent1", "concurrent1"])
    invoice2 = l2.rpc.call("invoice", [600000, "concurrent2", "concurrent2"])

    results = []

    def pay(invstring):
        try:
            l1.rpc.call("xpay", {"invstring": invstring, "maxfee": 1000})
            results.append("success")
        except RpcError as e:
            results.append(str(e))

    threads = [
        threading.Thread(target=pay, args=(invoice1["bolt11"],)),
        threading.Thread(target=pay, args=(invoice2["bolt11"],)),
    ]
    for t in threads:
        t.start()
    for t in threads:
        t.join(30)

    assert sorted(result == "success" for result in results) == [False, True]
    assert any("payany budget exceeded" in result for result in results)


def test_payany_too_many_args(node_factory, get_plugin):  # noqa: F811
    l1 = node_factory.get_node(
        options={"plugin": get_plugin, "log-level": "debug"},
    )

    with pytest.raises(RpcError, match="too many arguments given"):
        l1.rpc.call("payany", ["lno1qwerty", 1000, "msg", "extra"])
    assert not l1.daemon.is_in_log("panicked at")

    res = l1.rpc.call("payany", ["lno1qwerty", 1000])
    assert res["invoice"] == "lno1qwerty"


def test_setconfig_bad_params(node_factory, get_plugin):  # noqa: F811
    l1 = node_factory.get_node(
        options={"plugin": get_plugin, "log-level": "debug"},
    )

    with pytest.raises(RpcError, match="Unknown config option"):
        l1.rpc.call("setconfig", [1])

    with pytest.raises(RpcError, match="Unknown config option"):
        l1.rpc.call("setconfig", {"config": 1, "val": "true"})

    assert not l1.daemon.is_in_log("panicked at")

    res = l1.rpc.call("payany", ["lno1qwerty", 1000])
    assert res["invoice"] == "lno1qwerty"


def test_budget_amountless_invoice(node_factory, pay_renepay_deprecated, get_plugin):  # noqa: F811
    opts = {
        "plugin": get_plugin,
        "payany-budget-per": "5 hours",
        "payany-budget-amount-msat": 1000000,
        "payany-xpay-handle-pay": True,
        "log-level": "debug",
    }

    if pay_renepay_deprecated:
        opts["allow-deprecated-apis"] = True

    l1 = node_factory.get_node(options=opts)

    invoice = l1.rpc.call("invoice", ["any", "amountless1", "amountless1"])

    res = l1.rpc.call("xpay", {"invstring": invoice["bolt11"], "amount_msat": 1000})
    assert "timeout" not in res, f"no response for amountless invoice: {res}"
    assert not l1.daemon.is_in_log("panicked at")

    res = l1.rpc.call("payany", ["lno1qwerty", 1000])
    assert res["invoice"] == "lno1qwerty"

    invoice = l1.rpc.call("invoice", ["any", "amountless2", "amountless2"])

    res = l1.rpc.call("pay", {"bolt11": invoice["bolt11"], "amount_msat": 1000})
    assert "timeout" not in res, f"no response for amountless invoice: {res}"
    assert not l1.daemon.is_in_log("panicked at")


def test_bad_obj_param(node_factory, get_plugin):  # noqa: F811
    opts = {
        "plugin": get_plugin,
        "payany-budget-per": "5 hours",
        "payany-budget-amount-msat": 1000000,
        "log-level": "debug",
    }

    l1 = node_factory.get_node(
        options=opts,
    )

    with pytest.raises(RpcError, match="unknown `xpay` param: bolt11"):
        l1.rpc.call("xpay", {"bolt11": "lno1qwerty"})


def test_budget_time_period(node_factory, get_plugin):  # noqa: F811
    l1 = node_factory.get_node(
        options={"plugin": get_plugin, "log-level": "debug"},
    )

    with pytest.raises(RpcError):
        l1.rpc.call("setconfig", ["payany-budget-per", "garbage1week"])

    with pytest.raises(RpcError):
        l1.rpc.call("setconfig", ["payany-budget-per", "1week extra"])

    with pytest.raises(RpcError):
        l1.rpc.call("setconfig", ["payany-budget-per", "1.5h"])

    with pytest.raises(RpcError):
        l1.rpc.call("setconfig", ["payany-budget-per", "18446744073709551615w"])

    l1.rpc.call("setconfig", ["payany-budget-per", "18446744073709551615s"])

    l1.rpc.call("setconfig", ["payany-budget-per", "5 hours"])


def test_budget_bare_offer(node_factory, get_plugin):  # noqa: F811
    opts = [
        {
            "plugin": get_plugin,
            "payany-budget-per": "5 hours",
            "payany-budget-amount-msat": 1000000,
            "log-level": "debug",
        },
        {"log-level": "debug"},
    ]

    l1, l2 = node_factory.line_graph(
        2,
        wait_for_announce=True,
        opts=opts,
    )

    offer = l2.rpc.call("offer", {"amount": 1000, "description": "testpayany"})
    res = l1.rpc.call("xpay", {"invstring": offer["bolt12"]})
    assert res["amount_msat"] == 1000

    res = l1.rpc.call("xpay", {"invstring": offer["bolt12"], "amount_msat": 1200})
    assert res["amount_msat"] == 1200

    with pytest.raises(RpcError, match="amount_msat must be at least"):
        l1.rpc.call("xpay", {"invstring": offer["bolt12"], "amount_msat": 500})

    offer2 = l2.rpc.call("offer", {"amount": "any", "description": "testpayany2"})
    with pytest.raises(RpcError, match="Must specify amount"):
        l1.rpc.call("xpay", {"invstring": offer2["bolt12"]})

    res = l1.rpc.call("xpay", {"invstring": offer2["bolt12"], "amount_msat": 2000})
    assert res["amount_msat"] == 2000
