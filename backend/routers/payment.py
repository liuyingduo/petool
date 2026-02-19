"""
支付模块：微信支付 + 支付宝
支付成功后自动给用户充值 token 或延长会员有效期
"""
import os
import uuid
from datetime import datetime, timedelta
from typing import Optional

from bson import ObjectId
from fastapi import APIRouter, Depends, HTTPException, Request, Response
from fastapi.responses import JSONResponse

from auth_utils import get_current_user
from database import get_database
from schemas import CreateOrderRequest, CreateOrderResponse

router = APIRouter(prefix="/payment", tags=["支付"])

# ─── 套餐配置 ─────────────────────────────────────────────────────────────────

PLANS: dict[str, dict] = {
    "monthly": {
        "title": "专业版 - 月度订阅",
        "amount": 29.0,
        "token_grant": 0,      # 会员不单独发 token
        "days_grant": 30,
    },
    "yearly": {
        "title": "专业版 - 年度订阅",
        "amount": 299.0,
        "token_grant": 0,
        "days_grant": 365,
    },
    "pack-100w": {
        "title": "100万 Token 加油包",
        "amount": 9.9,
        "token_grant": 1_000_000,
        "days_grant": 0,
    },
    "pack-500w": {
        "title": "500万 Token 加油包",
        "amount": 39.9,
        "token_grant": 5_000_000,
        "days_grant": 0,
    },
}

# 套餐列表（给前端展示用）
@router.get("/plans", summary="获取可用套餐列表")
async def get_plans():
    return [
        {
            "id": plan_id,
            "title": info["title"],
            "amount": info["amount"],
            "token_grant": info["token_grant"],
            "days_grant": info["days_grant"],
        }
        for plan_id, info in PLANS.items()
    ]


# ─── 工具函数 ─────────────────────────────────────────────────────────────────

def _make_out_trade_no() -> str:
    """生成唯一商户订单号"""
    return f"PT{datetime.utcnow().strftime('%Y%m%d%H%M%S')}{uuid.uuid4().hex[:8].upper()}"


async def _fulfill_order(out_trade_no: str, transaction_id: str):
    """支付成功后兑现订单：充值 token / 延长会员"""
    db = get_database()
    order = await db.orders.find_one({"out_trade_no": out_trade_no})
    if not order or order["status"] == "paid":
        return  # 防重入

    plan = PLANS.get(order["plan_id"], {})
    token_grant = plan.get("token_grant", 0)
    days_grant = plan.get("days_grant", 0)

    user = await db.users.find_one({"_id": ObjectId(order["user_id"])})
    if not user:
        return

    inc_fields: dict = {}
    set_fields: dict = {"updated_at": datetime.utcnow()}

    if token_grant > 0:
        inc_fields["token_balance"] = token_grant

    if days_grant > 0:
        current_expire = user.get("membership_expire_at")
        if current_expire and current_expire > datetime.utcnow():
            new_expire = current_expire + timedelta(days=days_grant)
        else:
            new_expire = datetime.utcnow() + timedelta(days=days_grant)
        set_fields["membership_expire_at"] = new_expire
        set_fields["membership_level"] = "pro"

    update: dict = {"$set": set_fields}
    if inc_fields:
        update["$inc"] = inc_fields

    await db.users.update_one({"_id": ObjectId(order["user_id"])}, update)
    await db.orders.update_one(
        {"out_trade_no": out_trade_no},
        {"$set": {"status": "paid", "transaction_id": transaction_id, "paid_at": datetime.utcnow()}},
    )


# ─── 微信支付 ─────────────────────────────────────────────────────────────────

def _get_wechat_pay():
    try:
        from wechatpayv3 import WeChatPay, WeChatPayType
        mchid = os.getenv("WECHAT_PAY_MCHID", "")
        appid = os.getenv("WECHAT_PAY_APPID", "")
        api_v3_key = os.getenv("WECHAT_PAY_API_V3_KEY", "")
        private_key_path = os.getenv("WECHAT_PAY_PRIVATE_KEY_PATH", "")
        cert_serial = os.getenv("WECHAT_PAY_CERT_SERIAL_NO", "")

        if not all([mchid, appid, api_v3_key, private_key_path, cert_serial]):
            return None

        with open(private_key_path, "r") as f:
            private_key = f.read()

        return WeChatPay(
            wechatpay_type=WeChatPayType.NATIVE,
            mchid=mchid,
            private_key=private_key,
            cert_serial_no=cert_serial,
            appid=appid,
            apiv3_key=api_v3_key,
            notify_url=os.getenv("WECHAT_PAY_NOTIFY_URL", ""),
        )
    except Exception as e:
        print(f"[payment] 微信支付初始化失败: {e}")
        return None


@router.post("/wechat/create", response_model=CreateOrderResponse, summary="创建微信支付订单")
async def wechat_create(body: CreateOrderRequest, current_user: dict = Depends(get_current_user)):
    plan = PLANS.get(body.plan_id)
    if not plan:
        raise HTTPException(status_code=400, detail="无效套餐")

    out_trade_no = _make_out_trade_no()
    db = get_database()

    # 写入待支付订单
    await db.orders.insert_one({
        "user_id": current_user["_id"],
        "plan_id": body.plan_id,
        "title": plan["title"],
        "amount": plan["amount"],
        "token_grant": plan["token_grant"],
        "days_grant": plan["days_grant"],
        "payment_method": "wechat",
        "out_trade_no": out_trade_no,
        "status": "pending",
        "created_at": datetime.utcnow(),
    })

    wx_pay = _get_wechat_pay()
    if not wx_pay:
        # 开发模式：未配置微信支付时直接返回模拟二维码
        return CreateOrderResponse(
            out_trade_no=out_trade_no,
            payment_method="wechat",
            code_url=f"weixin://wxpay/bizpayurl?pr=MOCK_{out_trade_no}",
        )

    code, msg = wx_pay.pay(
        description=plan["title"],
        out_trade_no=out_trade_no,
        amount={"total": int(plan["amount"] * 100), "currency": "CNY"},
    )
    if code != 200:
        raise HTTPException(status_code=500, detail=f"微信支付下单失败: {msg}")

    import json as _json
    data = _json.loads(msg) if isinstance(msg, str) else msg
    return CreateOrderResponse(
        out_trade_no=out_trade_no,
        payment_method="wechat",
        code_url=data.get("code_url"),
    )


@router.post("/wechat/notify", summary="微信支付回调")
async def wechat_notify(request: Request):
    """微信支付成功回调（公网可访问）"""
    body = await request.body()
    headers = dict(request.headers)

    wx_pay = _get_wechat_pay()
    if not wx_pay:
        return Response(content="OK")

    result = wx_pay.callback(headers=headers, body=body)
    if result and result.get("event_type") == "TRANSACTION.SUCCESS":
        resource = result.get("resource", {})
        out_trade_no = resource.get("out_trade_no", "")
        transaction_id = resource.get("transaction_id", "")
        if out_trade_no:
            await _fulfill_order(out_trade_no, transaction_id)

    return Response(content='{"code":"SUCCESS","message":"成功"}', media_type="application/json")


# ─── 支付宝 ───────────────────────────────────────────────────────────────────

def _get_alipay():
    try:
        from alipay import AliPay  # alipay-sdk-python
        app_id = os.getenv("ALIPAY_APP_ID", "")
        private_key_path = os.getenv("ALIPAY_PRIVATE_KEY_PATH", "")
        public_key_path = os.getenv("ALIPAY_PUBLIC_KEY_PATH", "")
        sandbox = os.getenv("ALIPAY_SANDBOX", "true").lower() == "true"

        if not all([app_id, private_key_path, public_key_path]):
            return None

        with open(private_key_path) as f:
            private_key = f.read()
        with open(public_key_path) as f:
            public_key = f.read()

        return AliPay(
            appid=app_id,
            app_notify_url=os.getenv("ALIPAY_NOTIFY_URL", ""),
            app_private_key_string=private_key,
            alipay_public_key_string=public_key,
            sign_type="RSA2",
            debug=sandbox,
        )
    except Exception as e:
        print(f"[payment] 支付宝初始化失败: {e}")
        return None


@router.post("/alipay/create", response_model=CreateOrderResponse, summary="创建支付宝订单")
async def alipay_create(body: CreateOrderRequest, current_user: dict = Depends(get_current_user)):
    plan = PLANS.get(body.plan_id)
    if not plan:
        raise HTTPException(status_code=400, detail="无效套餐")

    out_trade_no = _make_out_trade_no()
    db = get_database()

    await db.orders.insert_one({
        "user_id": current_user["_id"],
        "plan_id": body.plan_id,
        "title": plan["title"],
        "amount": plan["amount"],
        "token_grant": plan["token_grant"],
        "days_grant": plan["days_grant"],
        "payment_method": "alipay",
        "out_trade_no": out_trade_no,
        "status": "pending",
        "created_at": datetime.utcnow(),
    })

    alipay = _get_alipay()
    if not alipay:
        return CreateOrderResponse(
            out_trade_no=out_trade_no,
            payment_method="alipay",
            pay_url=f"https://openapi.alipay.com/gateway.do?MOCK=1&out_trade_no={out_trade_no}",
        )

    order_string = alipay.api_alipay_trade_page_pay(
        out_trade_no=out_trade_no,
        total_amount=str(plan["amount"]),
        subject=plan["title"],
        return_url=os.getenv("ALIPAY_RETURN_URL", ""),
    )
    sandbox = os.getenv("ALIPAY_SANDBOX", "true").lower() == "true"
    gateway = "https://openapi-sandbox.dl.alipaydev.com/gateway.do" if sandbox else "https://openapi.alipay.com/gateway.do"
    pay_url = f"{gateway}?{order_string}"

    return CreateOrderResponse(
        out_trade_no=out_trade_no,
        payment_method="alipay",
        pay_url=pay_url,
    )


@router.post("/alipay/notify", summary="支付宝回调")
async def alipay_notify(request: Request):
    """支付宝支付成功异步通知"""
    form_data = await request.form()
    data = dict(form_data)
    sign = data.pop("sign", None)
    data.pop("sign_type", None)

    alipay = _get_alipay()
    if alipay and sign:
        success = alipay.verify(data, sign)
        if success and data.get("trade_status") in ("TRADE_SUCCESS", "TRADE_FINISHED"):
            out_trade_no = data.get("out_trade_no", "")
            trade_no = data.get("trade_no", "")
            if out_trade_no:
                await _fulfill_order(out_trade_no, trade_no)

    return Response(content="success")


# ─── 订单状态查询（前端轮询）────────────────────────────────────────────────────

@router.get("/order/{out_trade_no}", summary="查询订单支付状态")
async def query_order(out_trade_no: str, current_user: dict = Depends(get_current_user)):
    db = get_database()
    order = await db.orders.find_one({
        "out_trade_no": out_trade_no,
        "user_id": current_user["_id"],
    })
    if not order:
        raise HTTPException(status_code=404, detail="订单不存在")
    return {"status": order["status"], "out_trade_no": out_trade_no}


# ─── 🚧 开发专用：模拟支付成功 ────────────────────────────────────────────────
# 生产环境请在 .env 中设置 DEV_MOCK_PAY=false 来禁用此接口

@router.post(
    "/dev/mock-pay/{out_trade_no}",
    summary="[仅开发] 模拟支付成功",
    description="直接将订单标记为已支付，并执行充值/会员操作。**生产环境请设置 DEV_MOCK_PAY=false 禁用。**",
)
async def dev_mock_pay(out_trade_no: str, current_user: dict = Depends(get_current_user)):
    if os.getenv("DEV_MOCK_PAY", "true").lower() != "true":
        raise HTTPException(status_code=403, detail="此接口在生产环境中已禁用")

    db = get_database()
    order = await db.orders.find_one({
        "out_trade_no": out_trade_no,
        "user_id": current_user["_id"],
    })
    if not order:
        raise HTTPException(status_code=404, detail="订单不存在")
    if order["status"] == "paid":
        return {"message": "订单已经是已支付状态", "out_trade_no": out_trade_no}

    await _fulfill_order(out_trade_no, f"MOCK_TRANSACTION_{out_trade_no}")
    return {"message": "模拟支付成功，余额/会员已更新", "out_trade_no": out_trade_no}
