"""
AI 璇锋眰涓浆浠ｇ悊妯″潡锛圤penAI SDK 鐗堬級
- 瀹屽叏鍏煎 OpenAI Chat Completions API 鏍煎紡
- 浣跨敤 openai.AsyncOpenAI 瀹㈡埛绔浆鍙戝埌鍚勪笂娓稿巶鍟?
- 鏀寔娴佸紡锛坰tream=true锛夊拰闈炴祦寮忚姹?
- 鑷姩璁￠噺 Token锛屾墸鍑忕敤鎴蜂綑棰濓紝鍐欏叆鐢ㄩ噺璁板綍
"""
import asyncio
import json
import os
from datetime import datetime

import tiktoken
from bson import ObjectId
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import StreamingResponse
from openai import AsyncOpenAI

from auth_utils import get_current_user
from database import get_database

router = APIRouter(prefix="/v1", tags=["AI 涓浆浠ｇ悊"])

# 鈹€鈹€鈹€ 涓婃父鍘傚晢閰嶇疆锛堟湇鍔＄鎸佹湁 Key锛岀敤鎴蜂笉鍙锛夆攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

def _glm_client() -> AsyncOpenAI:
    return AsyncOpenAI(
        api_key=os.getenv("GLM_API_KEY", ""),
        base_url=os.getenv("GLM_API_BASE", "https://open.bigmodel.cn/api/paas/v4"),
    )

def _ark_client() -> AsyncOpenAI:
    return AsyncOpenAI(
        api_key=os.getenv("ARK_API_KEY", ""),
        base_url=os.getenv("ARK_API_BASE", "https://ark.cn-beijing.volces.com/api/v3"),
    )

def _minimax_client() -> AsyncOpenAI:
    # MiniMax 鍏煎 OpenAI 鏍煎紡锛屼娇鐢ㄥ叾 ChatCompletion 绔偣
    return AsyncOpenAI(
        api_key=os.getenv("MINIMAX_API_KEY", ""),
        base_url=os.getenv("MINIMAX_API_BASE", "https://api.minimax.chat/v1"),
        default_headers={"GroupId": os.getenv("MINIMAX_GROUP_ID", "")}
        if os.getenv("MINIMAX_GROUP_ID") else {},
    )

def _openai_client() -> AsyncOpenAI:
    return AsyncOpenAI(
        api_key=os.getenv("OPENAI_API_KEY", ""),
        base_url=os.getenv("OPENAI_API_BASE", "https://api.openai.com/v1"),
    )


def _get_client(model: str) -> AsyncOpenAI:
    """Resolve provider client by model name (case-insensitive)."""
    key = model.strip().lower()
    if key.startswith("minimax") or key.startswith("abab"):
        return _minimax_client()
    if key.startswith("doubao") or key.startswith("ep-"):   # ep-* 鏄?Ark endpoint ID
        return _ark_client()
    if key.startswith("glm"):
        return _glm_client()
    if key.startswith("gpt"):
        return _openai_client()
    # 榛樿璧?GLM
    return _glm_client()


# 鈹€鈹€鈹€ Token 璐圭巼琛紙key 缁熶竴灏忓啓锛夆攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

TOKEN_COST_MULTIPLIER: dict[str, float] = {
    "gpt-4o": 2.0,
    "gpt-4o-mini": 1.0,
    "glm-5": 1.0,
    "glm-4-plus": 1.5,
    "doubao-pro": 1.2,
    "doubao-lite": 0.8,
    "doubao-seed-1-6-thinking": 1.5,
    "minimax-m2.5": 1.0,
    "minimax-text-01": 1.0,
    "abab7-chat-preview": 1.5,
    "abab6.5s-chat": 1.0,
    "abab5.5-chat": 0.8,
}

def _get_cost_multiplier(model: str) -> float:
    return TOKEN_COST_MULTIPLIER.get(model.strip().lower(), 1.0)


# 鈹€鈹€鈹€ Token 璁￠噺 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

def _count_tokens_approx(text: str) -> int:
    try:
        enc = tiktoken.get_encoding("cl100k_base")
        return len(enc.encode(text))
    except Exception:
        return max(1, len(text) // 4)


def _extract_text_from_messages(messages: list) -> str:
    parts = []
    for m in messages:
        content = m.get("content") or ""
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get("type") == "text":
                    parts.append(block.get("text", ""))
        else:
            parts.append(str(content))
    return "\n".join(parts)


async def _check_balance(user_id: str, estimated_cost: int):
    db = get_database()
    user = await db.users.find_one({"_id": ObjectId(user_id)}, {"token_balance": 1})
    if not user or user.get("token_balance", 0) < max(1, estimated_cost):
        raise HTTPException(status_code=402, detail="Insufficient token balance")


async def _deduct_tokens(
    user_id: str, model: str,
    prompt_tokens: int, completion_tokens: int,
    task_type: str = "瀵硅瘽"
):
    db = get_database()
    total = prompt_tokens + completion_tokens
    cost = max(1, int(total * _get_cost_multiplier(model)))
    now = datetime.utcnow()
    await db.usage_records.insert_one({
        "user_id": user_id,
        "model": model,
        "task_type": task_type,
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": total,
        "cost_tokens": cost,
        "created_at": now,
    })
    await db.users.update_one(
        {"_id": ObjectId(user_id)},
        {"$inc": {"token_balance": -cost, "token_total_used": cost}, "$set": {"updated_at": now}},
    )


# 鈹€鈹€鈹€ 涓昏矾鐢憋細POST /v1/chat/completions 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

@router.post("/chat/completions", summary="Chat completions proxy")
async def chat_completions(request: Request, current_user: dict = Depends(get_current_user)):
    user_id = current_user["_id"]
    body = await request.json()

    model: str = body.get("model", "glm-5")
    messages: list = body.get("messages", [])
    is_stream: bool = body.get("stream", False)
    model_key = model.strip().lower()

    passthrough_kwargs = {
        k: v for k, v in body.items() if k not in ("model", "messages", "stream")
    }
    # MiniMax-M2.5: enable interleaved thinking split for easier downstream rendering.
    if model_key == "minimax-m2.5":
        extra_body = passthrough_kwargs.get("extra_body")
        extra_body_payload = extra_body.copy() if isinstance(extra_body, dict) else {}
        extra_body_payload["reasoning_split"] = True
        passthrough_kwargs["extra_body"] = extra_body_payload

    # 棰勪及杈撳叆 token 骞舵鏌ヤ綑棰?
    prompt_tokens = _count_tokens_approx(_extract_text_from_messages(messages))
    await _check_balance(user_id, prompt_tokens)

    client = _get_client(model)

    # 鈹€鈹€ 娴佸紡鍝嶅簲 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
    if is_stream:
        async def event_generator():
            completion_text = ""
            try:
                async with client.chat.completions.with_streaming_response.create(
                    model=model,
                    messages=messages,
                    stream=True,
                    **passthrough_kwargs,
                ) as upstream:
                    async for line in upstream.iter_lines():
                        # 原样转发上游 SSE，避免中间层改写字段（如 reasoning_content）
                        yield f"{line}\n"

                        # 收集 completion 文本用于事后计量
                        if not line.startswith("data:"):
                            continue
                        data_raw = line[5:].strip()
                        if not data_raw or data_raw == "[DONE]":
                            continue
                        try:
                            payload = json.loads(data_raw)
                        except Exception:
                            continue
                        for choice in payload.get("choices", []):
                            delta = choice.get("delta") if isinstance(choice, dict) else None
                            if not isinstance(delta, dict):
                                continue
                            content = delta.get("content")
                            if isinstance(content, str):
                                completion_text += content
            except Exception as e:
                error_payload = json.dumps({"error": {"message": str(e), "type": "upstream_error"}})
                yield f"data: {error_payload}\n\n"
                return
            finally:
                comp_tokens = _count_tokens_approx(completion_text)
                asyncio.create_task(_deduct_tokens(user_id, model, prompt_tokens, comp_tokens))

        return StreamingResponse(event_generator(), media_type="text/event-stream")

    # 鈹€鈹€ 闈炴祦寮忓搷搴?鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
    try:
        response = await client.chat.completions.create(
            model=model,
            messages=messages,
            stream=False,
            **passthrough_kwargs,
        )
    except Exception as e:
        raise HTTPException(status_code=502, detail=f"涓婃父璇锋眰澶辫触: {e}")

    usage = response.usage
    comp_tokens = usage.completion_tokens if usage else _count_tokens_approx(
        response.choices[0].message.content or "" if response.choices else ""
    )
    asyncio.create_task(_deduct_tokens(user_id, model, prompt_tokens, comp_tokens))

    return response.model_dump(exclude_unset=True)

