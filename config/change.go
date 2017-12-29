package config

import (
	"flag"
	"fmt"
	"log"
	"strings"
	"time"
)

var (
	changeApplyDelay = flag.Duration("change-apply-delay", 100*time.Millisecond, "Delay before applying change in Kubernetes configuration")
	customBackends   = flag.String("custom", "", "Custom backend definitions (format: \"<host><path>:<target IP>:<target port>,...\")")

	changeNum        uint64 = 0
	appliedChangeNum uint64 = 0
)

type NewConfigFunc func() Config

func NotifyChanged(callback NewConfigFunc) {
	changeNum += 1
	go applyChange(changeNum, callback)
}

// Wait a bit and apply the changes
func applyChange(myChangeNum uint64, callback NewConfigFunc) {
	time.Sleep(*changeApplyDelay)

	if appliedChangeNum >= myChangeNum {
		return // already applied
	}

	Lock()
	defer Unlock()

	if appliedChangeNum >= myChangeNum {
		return // already applied
	}

	config := callback()

	if len(*customBackends) != 0 {
		for idx, be := range strings.Split(*customBackends, ",") {
			parts := strings.Split(be, ":")

			if len(parts) != 3 {
				log.Fatal("bad custom backend format: %s", be)
			}

			hostParts := strings.SplitN(parts[0], "/", 2)
			host := hostParts[0]
			path := ""
			if len(hostParts) > 1 {
				path = "/" + hostParts[1]
			}

			target := parts[1] + ":" + parts[2]

			config.HostBackends[host] = []*Backend{
				NewBackend(fmt.Sprintf("custom[%d]", idx), path, target),
			}
		}
	}

	Current = &config

	appliedChangeNum = changeNum
}
