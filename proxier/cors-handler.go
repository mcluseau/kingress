package proxier

/* TODO
import (
	"net"
	"net/http"
)

var (
	// OriginNotAllowedResponse is the response sent to a client when a CORS origin is not allowed
	OriginNotAllowedResponse = HTTPStatusResponse(http.StatusForbidden, "origin not allowed")
)

type CORSHandler struct {
	AllowedOrigins []string
}

func (h CORSHandler) Handle(req Request, alreadyRead []byte, src net.Conn) (handled bool) {
	var origin, accessControlRequestMethod string

	ParseHeader(alreadyRead, func(hdr string, v []byte) bool {
		switch hdr {
		case "Origin":
			origin = string(v)
		case "Access-Control-Request-Method":
			accessControlRequestMethod = string(v)
		}
		return origin == "" || accessControlRequestMethod == ""
	})

	if origin != "" {
		allowed := len(h.AllowedOrigins) == 0
		if !allowed {
			for _, allowedOrigin := range h.AllowedOrigins {
				if origin == allowedOrigin {
					allowed = true
					break
				}
			}
		}
		if !allowed {
			src.Write(OriginNotAllowedResponse)
			return
		}

		//hdr.Set("Access-Control-Allow-Origin", origin)
	}

	if req.Method == http.MethodOptions {
		handled = true

	}

	return
}
*/
