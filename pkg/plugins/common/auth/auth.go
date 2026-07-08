package auth

type Auth string

const (
	APIKey Auth = "apikey"
	Simple Auth = "simple"
	SigV4  Auth = "sigv4"
	OAuth2 Auth = "oauth2"
)

const (
	APIKeyAuthHeaderName  = "authHeaderName"
	APIKeyAuthValuePrefix = "authValuePrefix"
)
