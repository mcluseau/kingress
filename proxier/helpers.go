package proxier

import (
	"bytes"
	"net/textproto"
)

func HeaderValue(data []byte, headerName string) (value string) {
	ParseHeader(data, func(hdr string, v []byte) (cont bool) {
		if hdr == headerName {
			value = string(v)
			return false
		}
		return true
	})
	return
}

func ParseHeader(data []byte, headerFunc func(hdr string, value []byte) (cont bool)) {
	line, data, found := bytes.Cut(data, []byte{'\n'})
	if !found {
		return
	}

	for {
		line, data, found = bytes.Cut(data, []byte{'\n'})
		if !found {
			return
		}

		if len(line) == 0 {
			return
		}

		if line[len(line)-1] == '\r' {
			line = line[:len(line)-1]
		}

		h, v, found := bytes.Cut(line, []byte{':'})
		if !found {
			continue
		}

		for len(v) != 0 && v[0] == ' ' {
			v = v[1:]
		}

		// TODO handle line continuation

		cont := headerFunc(textproto.CanonicalMIMEHeaderKey(string(h)), v)
		if !cont {
			break
		}
	}

	return
}
