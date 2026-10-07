FROM node:18-alpine as build
WORKDIR /app
COPY package*.json ./
RUN npm ci --legacy-peer-deps
COPY . .
RUN npm run build

FROM node:18-alpine
WORKDIR /app
RUN npm install -g serve \
    && addgroup -g 10001 guiuser \
    && adduser -u 10001 -G guiuser -D guiuser

COPY --from=build /app/dist ./dist

# Script to inject environment variables at runtime
RUN printf '#!/bin/sh\necho "window._env_ = { GHOSTLINK_API_BASE: \"$GHOSTLINK_API_BASE\" };" > /app/dist/env-config.js\nexec serve -s dist -l 5173\n' > /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh && chown -R guiuser:guiuser /app

USER guiuser

ENV GHOSTLINK_API_BASE=http://127.0.0.1:8000
EXPOSE 5173

ENTRYPOINT ["/app/entrypoint.sh"]
