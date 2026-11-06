
package main

// Minimal same-origin front for the private rekor-tiles deployment:
//   /api/v2/* -> write API (127.0.0.1:5232)
//   /*        -> static tile store (127.0.0.1:5235)
// Mirrors the production shape where one hostname serves both the write API
// and the tlog-tiles read path (see CLIENTS.md / public-good instance).
import (
	"log"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"
)

func main() {
	writeURL, _ := url.Parse("http://127.0.0.1:5232")
	readURL, _ := url.Parse("http://127.0.0.1:5235")
	writeProxy := httputil.NewSingleHostReverseProxy(writeURL)
	readProxy := httputil.NewSingleHostReverseProxy(readURL)
	handler := func(w http.ResponseWriter, r *http.Request) {
		toWrite := strings.HasPrefix(r.URL.Path, "/api/v2/")
		log.Printf("%s %s -> %s", r.Method, r.URL.Path, map[bool]string{true: "write", false: "read"}[toWrite])
		if toWrite {
			writeProxy.ServeHTTP(w, r)
			return
		}
		readProxy.ServeHTTP(w, r)
	}
	log.Println("proxy listening on 127.0.0.1:5239")
	log.Fatal(http.ListenAndServe("127.0.0.1:5239", http.HandlerFunc(handler)))
}
