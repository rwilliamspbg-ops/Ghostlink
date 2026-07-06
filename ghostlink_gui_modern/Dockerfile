FROM node:18-alpine as build

WORKDIR /app

COPY package*.json ./
RUN npm ci

COPY . .
RUN npm run build

FROM node:18-alpine

WORKDIR /app

RUN npm install -g serve

COPY --from=build /app/dist ./dist

ENV GHOSTLINK_API_BASE=http://127.0.0.1:8003
EXPOSE 3000

CMD ["serve", "-s", "dist", "-l", "3000"]
