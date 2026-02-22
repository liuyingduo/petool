"""
Baidu OCR API Router
"""
import os
import base64
import urllib.parse
from fastapi import APIRouter, HTTPException, Depends
from pydantic import BaseModel
import httpx

from auth_utils import get_current_user

router = APIRouter(prefix="/v1/ocr", tags=["OCR API"])

class OCRRequest(BaseModel):
    image: str  # Base64 encoded image

async def get_access_token() -> str:
    """
    使用 AK，SK 生成鉴权签名（Access Token）
    """
    api_key = os.getenv("BAIDU_OCR_API_KEY", "")
    secret_key = os.getenv("BAIDU_OCR_SECRET_KEY", "")
    
    if not api_key or not secret_key:
        raise HTTPException(status_code=500, detail="Baidu OCR API credentials not configured.")

    url = "https://aip.baidubce.com/oauth/2.0/token"
    params = {
        "grant_type": "client_credentials",
        "client_id": api_key,
        "client_secret": secret_key
    }
    
    async with httpx.AsyncClient() as client:
        response = await client.post(url, params=params)
        response.raise_for_status()
        data = response.json()
        token = data.get("access_token")
        if not token:
            raise HTTPException(status_code=500, detail="Failed to get Baidu OCR access token.")
        return str(token)

@router.post("/general", summary="General OCR with Location")
async def general_ocr(request: OCRRequest, current_user: dict = Depends(get_current_user)):
    """
    Call Baidu General OCR API with vertexes_location=true
    """
    token = await get_access_token()
    url = f"https://aip.baidubce.com/rest/2.0/ocr/v1/general?access_token={token}"
    
    # URL encode the base64 image
    image_encoded = urllib.parse.quote_plus(request.image)
    
    payload = f"image={image_encoded}&detect_direction=false&detect_language=false&vertexes_location=true&paragraph=false&probability=false"
    headers = {
        'Content-Type': 'application/x-www-form-urlencoded',
        'Accept': 'application/json'
    }
    
    async with httpx.AsyncClient() as client:
        try:
            response = await client.post(url, headers=headers, content=payload.encode("utf-8"))
            response.raise_for_status()
            
            # response.encoding = "utf-8" in httpx is automatic for json
            return response.json()
        except httpx.HTTPError as e:
            raise HTTPException(status_code=502, detail=f"Baidu OCR request failed: {e}")
