from datetime import datetime, date, timedelta
from fastapi import APIRouter, Depends, Query
from bson import ObjectId
from database import get_database
from auth_utils import get_current_user
from schemas import (
    UserProfile,
    QuotaDashboard,
    UsageRecordOut,
    UsagePage,
    OrderOut,
)

router = APIRouter(prefix="/account", tags=["账户"])


def _days_left(expire_at: datetime | None) -> int:
    if not expire_at:
        return 0
    delta = expire_at - datetime.utcnow()
    return max(0, delta.days)


@router.get("/profile", response_model=UserProfile, summary="获取个人资料")
async def get_profile(current_user: dict = Depends(get_current_user)):
    balance = current_user.get("token_balance", 0)
    total_used = current_user.get("token_total_used", 0)
    total_ever = balance + total_used
    usage_percent = round((total_used / total_ever * 100) if total_ever > 0 else 0, 1)

    expire_at = current_user.get("membership_expire_at")

    return UserProfile(
        user_id=current_user["_id"],
        username=current_user["username"],
        email=current_user["email"],
        avatar=current_user.get("avatar"),
        membership_level=current_user.get("membership_level", "free"),
        membership_expire_at=expire_at,
        days_left=_days_left(expire_at),
        token_balance=balance,
        token_total_used=total_used,
        token_usage_percent=usage_percent,
    )


@router.get("/quota", response_model=QuotaDashboard, summary="余额仪表盘 + 7 天趋势")
async def get_quota(current_user: dict = Depends(get_current_user)):
    db = get_database()
    user_id = current_user["_id"]

    # 今日消耗
    today_start = datetime.utcnow().replace(hour=0, minute=0, second=0, microsecond=0)
    pipeline_today = [
        {"$match": {"user_id": user_id, "created_at": {"$gte": today_start}}},
        {"$group": {"_id": None, "total": {"$sum": "$cost_tokens"}}},
    ]
    today_result = await db.usage_records.aggregate(pipeline_today).to_list(1)
    consumed_today = today_result[0]["total"] if today_result else 0

    # 近 7 天趋势
    trend = []
    for i in range(6, -1, -1):
        day_start = datetime.utcnow().replace(hour=0, minute=0, second=0, microsecond=0) - timedelta(days=i)
        day_end = day_start + timedelta(days=1)
        pipeline_day = [
            {"$match": {"user_id": user_id, "created_at": {"$gte": day_start, "$lt": day_end}}},
            {"$group": {"_id": None, "total": {"$sum": "$cost_tokens"}}},
        ]
        result = await db.usage_records.aggregate(pipeline_day).to_list(1)
        trend.append({
            "date": day_start.strftime("%m-%d"),
            "value": result[0]["total"] if result else 0,
        })

    return QuotaDashboard(
        total_balance=current_user.get("token_balance", 0),
        consumed_today=consumed_today,
        trend=trend,
    )


@router.get("/usage", response_model=UsagePage, summary="消费明细（分页）")
async def get_usage(
    page: int = Query(1, ge=1),
    page_size: int = Query(10, ge=1, le=100),
    current_user: dict = Depends(get_current_user),
):
    db = get_database()
    user_id = current_user["_id"]
    skip = (page - 1) * page_size

    total = await db.usage_records.count_documents({"user_id": user_id})
    cursor = (
        db.usage_records.find({"user_id": user_id})
        .sort("created_at", -1)
        .skip(skip)
        .limit(page_size)
    )
    records = []
    async for doc in cursor:
        records.append(UsageRecordOut(
            id=str(doc["_id"]),
            created_at=doc["created_at"].strftime("%Y-%m-%d %H:%M"),
            task_type=doc.get("task_type", "对话"),
            model=doc.get("model", ""),
            prompt_tokens=doc.get("prompt_tokens", 0),
            completion_tokens=doc.get("completion_tokens", 0),
            total_tokens=doc.get("total_tokens", 0),
            cost_tokens=doc.get("cost_tokens", 0),
        ))

    return UsagePage(records=records, total=total, page=page, page_size=page_size)


@router.get("/orders", response_model=list[OrderOut], summary="订单列表")
async def get_orders(current_user: dict = Depends(get_current_user)):
    db = get_database()
    user_id = current_user["_id"]
    cursor = db.orders.find({"user_id": user_id}).sort("created_at", -1).limit(50)
    orders = []
    async for doc in cursor:
        orders.append(OrderOut(
            id=str(doc["_id"]),
            title=doc.get("title", ""),
            amount=doc.get("amount", 0),
            plan_id=doc.get("plan_id", ""),
            payment_method=doc.get("payment_method", ""),
            status=doc.get("status", "pending"),
            created_at=doc["created_at"].strftime("%Y-%m-%d %H:%M"),
        ))
    return orders
