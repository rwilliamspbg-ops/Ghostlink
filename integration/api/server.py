"""
FastAPI-based REST API server for the Mohawk Inference Engine.

Provides OpenAI-compatible endpoints for:
- Text completions
- Chat completions
- Model management
- Health checks
"""

import asyncio
import json
import logging
from typing import Optional, List, Dict, Any, AsyncGenerator
from dataclasses import dataclass

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import StreamingResponse, JSONResponse
from pydantic import BaseModel, Field

from ..engine import InferenceEngine, InferenceResult

logger = logging.getLogger(__name__)


# Request/Response Models
class CompletionRequest(BaseModel):
    """OpenAI-compatible completion request"""
    prompt: str
    model: Optional[str] = None
    max_tokens: int = Field(default=100, ge=1, le=4096)
    temperature: float = Field(default=0.7, ge=0.0, le=2.0)
    top_p: float = Field(default=0.9, ge=0.0, le=1.0)
    stop: Optional[List[str]] = None
    stream: bool = False


class CompletionChoice(BaseModel):
    """Completion choice in response"""
    text: str
    index: int = 0
    finish_reason: Optional[str] = None


class UsageInfo(BaseModel):
    """Token usage information"""
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0


class CompletionResponse(BaseModel):
    """OpenAI-compatible completion response"""
    id: str
    object: str = "text_completion"
    created: int
    model: str
    choices: List[CompletionChoice]
    usage: Optional[UsageInfo] = None


class ChatMessage(BaseModel):
    """Chat message"""
    role: str
    content: str


class ChatCompletionRequest(BaseModel):
    """OpenAI-compatible chat completion request"""
    messages: List[ChatMessage]
    model: Optional[str] = None
    max_tokens: int = Field(default=100, ge=1, le=4096)
    temperature: float = Field(default=0.7, ge=0.0, le=2.0)
    top_p: float = Field(default=0.9, ge=0.0, le=1.0)
    stop: Optional[List[str]] = None
    stream: bool = False


class ChatCompletionChoice(BaseModel):
    """Chat completion choice"""
    message: ChatMessage
    index: int = 0
    finish_reason: Optional[str] = None


class ChatCompletionResponse(BaseModel):
    """OpenAI-compatible chat completion response"""
    id: str
    object: str = "chat.completion"
    created: int
    model: str
    choices: List[ChatCompletionChoice]
    usage: Optional[UsageInfo] = None


class ModelInfo(BaseModel):
    """Model information"""
    id: str
    object: str = "model"
    created: int
    owned_by: str = "mohawk"


class ModelList(BaseModel):
    """List of available models"""
    object: str = "list"
    data: List[ModelInfo]


class APIServer:
    """
    REST API Server for Mohawk Inference Engine.
    
    Provides OpenAI-compatible endpoints for seamless integration.
    """
    
    def __init__(self, engine: Optional[InferenceEngine] = None, host: str = "0.0.0.0", port: int = 8080):
        """
        Initialize the API server.
        
        Args:
            engine: InferenceEngine instance (creates one if not provided)
            host: Host to bind to
            port: Port to listen on
        """
        self.engine = engine or InferenceEngine()
        self.host = host
        self.port = port
        self.app = FastAPI(
            title="Mohawk Inference Engine",
            description="High-performance local LLM inference API",
            version="0.1.0",
        )
        self._setup_routes()
    
    def _setup_routes(self):
        """Set up all API routes"""
        
        @self.app.get("/")
        async def root():
            """Root endpoint with API info"""
            return {
                "name": "Mohawk Inference Engine",
                "version": "0.1.0",
                "status": "running",
            }
        
        @self.app.get("/health")
        async def health_check():
            """Health check endpoint"""
            return {"status": "healthy"}
        
        @self.app.get("/v1/models", response_model=ModelList)
        async def list_models():
            """List available models"""
            info = self.engine.get_info()
            model_id = info.get("model_path", "default") or "default"
            return ModelList(
                data=[
                    ModelInfo(
                        id=model_id,
                        created=0,
                    )
                ]
            )
        
        @self.app.post("/v1/completions")
        async def create_completion(request: CompletionRequest):
            """Create a text completion"""
            try:
                if request.stream:
                    return StreamingResponse(
                        self._stream_completion(request),
                        media_type="text/event-stream",
                    )
                else:
                    return await self._completion(request)
            except Exception as e:
                logger.error(f"Completion error: {e}")
                raise HTTPException(status_code=500, detail=str(e))
        
        @self.app.post("/v1/chat/completions")
        async def create_chat_completion(request: ChatCompletionRequest):
            """Create a chat completion"""
            try:
                # Convert chat messages to prompt
                prompt = self._format_chat_prompt(request.messages)
                
                # Create completion request
                comp_request = CompletionRequest(
                    prompt=prompt,
                    model=request.model,
                    max_tokens=request.max_tokens,
                    temperature=request.temperature,
                    top_p=request.top_p,
                    stop=request.stop,
                    stream=request.stream,
                )
                
                if request.stream:
                    return StreamingResponse(
                        self._stream_chat_completion(comp_request),
                        media_type="text/event-stream",
                    )
                else:
                    result = await self._completion(comp_request)
                    # Convert to chat format
                    first_choice = result.choices[0]
                    return ChatCompletionResponse(
                        id=result.id,
                        created=result.created,
                        model=result.model,
                        choices=[
                            ChatCompletionChoice(
                                message=ChatMessage(role="assistant", content=first_choice.text),
                                index=first_choice.index,
                                finish_reason=first_choice.finish_reason,
                            )
                        ],
                        usage=result.usage,
                    )
            except Exception as e:
                logger.error(f"Chat completion error: {e}")
                raise HTTPException(status_code=500, detail=str(e))
    
    def _format_chat_prompt(self, messages: List[ChatMessage]) -> str:
        """Format chat messages into a single prompt"""
        formatted = []
        for msg in messages:
            formatted.append(f"{msg.role}: {msg.content}")
        return "\n".join(formatted) + "\nassistant:"
    
    async def _completion(self, request: CompletionRequest) -> CompletionResponse:
        """Handle non-streaming completion request"""
        import time
        import uuid
        
        result = self.engine.generate(
            prompt=request.prompt,
            max_tokens=request.max_tokens,
            temperature=request.temperature,
            top_p=request.top_p,
            stop_sequences=request.stop,
        )
        
        return CompletionResponse(
            id=f"cmpl-{uuid.uuid4().hex[:8]}",
            created=int(time.time()),
            model=result.model_name,
            choices=[
                CompletionChoice(
                    text=result.text,
                    index=0,
                    finish_reason="stop",
                )
            ],
            usage=UsageInfo(
                completion_tokens=result.tokens_generated,
            ),
        )
    
    async def _stream_completion(self, request: CompletionRequest) -> AsyncGenerator[str, None]:
        """Handle streaming completion request"""
        import time
        import uuid
        
        generator = self.engine.generate(
            prompt=request.prompt,
            max_tokens=request.max_tokens,
            temperature=request.temperature,
            top_p=request.top_p,
            stop_sequences=request.stop,
            stream=True,
        )
        
        chunk_id = f"cmpl-{uuid.uuid4().hex[:8]}"
        created = int(time.time())
        
        async for token in generator:
            chunk = {
                "id": chunk_id,
                "object": "text_completion.chunk",
                "created": created,
                "model": request.model or "default",
                "choices": [
                    {"text": token, "index": 0, "finish_reason": None}
                ],
            }
            yield f"data: {json.dumps(chunk)}\n\n"
        
        # Final chunk
        final_chunk = {
            "id": chunk_id,
            "object": "text_completion.chunk",
            "created": created,
            "model": request.model or "default",
            "choices": [
                {"text": "", "index": 0, "finish_reason": "stop"}
            ],
        }
        yield f"data: {json.dumps(final_chunk)}\n\n"
        yield "data: [DONE]\n\n"
    
    async def _stream_chat_completion(self, request: CompletionRequest) -> AsyncGenerator[str, None]:
        """Handle streaming chat completion request"""
        import time
        import uuid
        
        generator = self.engine.generate(
            prompt=request.prompt,
            max_tokens=request.max_tokens,
            temperature=request.temperature,
            top_p=request.top_p,
            stop_sequences=request.stop,
            stream=True,
        )
        
        chunk_id = f"chatcmpl-{uuid.uuid4().hex[:8]}"
        created = int(time.time())
        
        async for token in generator:
            chunk = {
                "id": chunk_id,
                "object": "chat.completion.chunk",
                "created": created,
                "model": request.model or "default",
                "choices": [
                    {
                        "delta": {"content": token},
                        "index": 0,
                        "finish_reason": None,
                    }
                ],
            }
            yield f"data: {json.dumps(chunk)}\n\n"
        
        # Final chunk
        final_chunk = {
            "id": chunk_id,
            "object": "chat.completion.chunk",
            "created": created,
            "model": request.model or "default",
            "choices": [
                {"delta": {}, "index": 0, "finish_reason": "stop"}
            ],
        }
        yield f"data: {json.dumps(final_chunk)}\n\n"
        yield "data: [DONE]\n\n"
    
    def run(self, host: Optional[str] = None, port: Optional[int] = None):
        """
        Run the API server.
        
        Args:
            host: Override host (uses default if None)
            port: Override port (uses default if None)
        """
        import uvicorn
        
        host = host or self.host
        port = port or self.port
        
        logger.info(f"Starting Mohawk API server on {host}:{port}")
        uvicorn.run(self.app, host=host, port=port, log_level="info")
