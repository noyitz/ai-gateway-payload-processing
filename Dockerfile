FROM golang:1.25 AS builder
WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -ldflags="-w -s" -o /bbr-server ./cmd/main.go

FROM gcr.io/distroless/static:nonroot
COPY --from=builder /bbr-server /bbr-server
EXPOSE 9004 9005 9090
ENTRYPOINT ["/bbr-server"]
