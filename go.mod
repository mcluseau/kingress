module github.com/mcluseau/kingress

require (
	github.com/googleapis/gnostic v0.5.1 // indirect
	github.com/hashicorp/golang-lru v0.5.4 // indirect
	github.com/imdario/mergo v0.3.10 // indirect
	github.com/mailgun/timetools v0.0.0-20170619190023-f3a7b8ffff47 // indirect
	github.com/sirupsen/logrus v1.6.0 // indirect
	github.com/vulcand/oxy v1.1.0
	google.golang.org/appengine v1.6.6 // indirect
	gopkg.in/mgo.v2 v2.0.0-20180705113604-9856a29383ce // indirect
	k8s.io/api v0.20.1
	k8s.io/apimachinery v0.20.1
	k8s.io/client-go v1.5.1
)

go 1.14

replace k8s.io/client-go => k8s.io/client-go v0.20.1
