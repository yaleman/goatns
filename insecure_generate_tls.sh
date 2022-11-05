#!/bin/sh

set -e

# you can set the hostname if you want, but it'll default to localhost
if [ -z "$CERT_HOSTNAME" ]; then
    CERT_HOSTNAME="localhost"
fi

# also where the files are stored
if [ -z "$CERT_DIR" ]; then
    CERT_DIR=./certificates/
fi

# days
CERT_LIFETIME=91

#ensure it's got a trailing slash by stripping it then adding it again for great justice.
CERT_DIR="${CERT_DIR%/}/"
# echo "CERT DIR: ${CERT_DIR}"

ALTNAME_FILE="${CERT_DIR}altnames.cnf"
CANAME_FILE="${CERT_DIR}ca.cnf"
CACERT="${CERT_DIR}ca.pem"
CAKEY="${CERT_DIR}cakey.pem"
CADB="${CERT_DIR}ca.txt"
CASRL="${CERT_DIR}ca.srl"

KEYFILE="${CERT_DIR}key.pem"
CERTFILE="${CERT_DIR}cert.pem"
CSRFILE="${CERT_DIR}cert.csr"
CHAINFILE="${CERT_DIR}chain.pem"
# DHFILE="${CERT_DIR}dh.pem"

# echo "Cleaning up certificate dir..."
# rm ${CERT_DIR}*

if [ ! -d "${CERT_DIR}" ]; then
    echo "Creating cert dir: ${CERT_DIR}"
    mkdir -p "${CERT_DIR}"
fi

cat > "${CANAME_FILE}" << DEVEOF
[req]
nsComment = "Certificate Authority"
distinguished_name  = req_distinguished_name
req_extensions = v3_ca

[ req_distinguished_name ]

countryName                     = Country Name (2 letter code)
countryName_default             = GO
countryName_min                 = 2
countryName_max                 = 2

stateOrProvinceName             = State or Province Name (full name)
stateOrProvinceName_default     = Goatsland

localityName                    = Locality Name (eg, city)
localityName_default            = GoatVille

0.organizationName              = Organization Name (eg, company)
0.organizationName_default      = Goat Dot Net

organizationalUnitName          = Organizational Unit Name (eg, section)
organizationalUnitName_default =  GoatNS

commonName                      = Common Name (eg, your name or your server\'s hostname)
commonName_max                  = 64
commonName_default              = insecure.ca.goat

[ v3_ca ]
subjectKeyIdentifier = hash
basicConstraints = critical, CA:true
keyUsage = critical, digitalSignature, cRLSign, keyCertSign

DEVEOF

cat > "${ALTNAME_FILE}" << DEVEOF

[ca]
default_ca = CA_default

[ CA_default ]
# Directory and file locations.
dir               = ${CERT_DIR}
certs             = ${CERT_DIR}
crl_dir           = ${CERT_DIR}
new_certs_dir     = ${CERT_DIR}
database          = ${CADB}
serial            = ${CASRL}

# The root key and root certificate.
private_key       = ${CAKEY}
certificate       = ${CACERT}

# SHA-1 is deprecated, so use SHA-2 instead.
default_md        = sha256

name_opt          = ca_default
cert_opt          = ca_default
default_days      = 3650
preserve          = no
policy            = policy_loose

[ policy_loose ]
countryName             = optional
stateOrProvinceName     = optional
localityName            = optional
organizationName        = optional
organizationalUnitName  = optional
commonName              = supplied
emailAddress            = optional

[req]
nsComment = "Certificate"
distinguished_name  = req_distinguished_name
req_extensions = v3_req
unique_subject = false

[ req_distinguished_name ]

countryName                     = Country Name (2 letter code)
countryName_default             = GO
countryName_min                 = 2
countryName_max                 = 2

stateOrProvinceName             = State or Province Name (full name)
stateOrProvinceName_default     = GoatsLand

localityName                    = Locality Name (eg, city)
localityName_default            = GoatVille

0.organizationName              = Organization Name (eg, company)
0.organizationName_default      = Goat Dot Net

organizationalUnitName          = Organizational Unit Name (eg, section)
organizationalUnitName_default =  GoatNS

commonName                      = Common Name (eg, your name or your server\'s hostname)
commonName_max                  = 255
commonName_default              = ${CERT_HOSTNAME}

[ v3_req ]
basicConstraints = CA:FALSE
nsCertType = server
nsComment = "Server Certificate"
subjectKeyIdentifier = hash
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
IP.1 = 127.0.0.1

DEVEOF

# because we can't generate duplicates and meh
rm "${CADB}"

touch ${CADB}

echo 1000 > ${CASRL}

if [ ! -f "${CAKEY}" ]; then
    echo "Make the CA key..."
    openssl ecparam -genkey -name prime256v1 -noout -out "${CAKEY}"|| { echo "Failed to gen the CA key..."; exit 1 ;}
fi

if [ ! -f "${CACERT}" ]; then
    echo "Self sign the CA..."
    openssl req -batch -config "${CANAME_FILE}" \
        -key "${CAKEY}" \
        -new -x509 -days +${CERT_LIFETIME} \
        -sha256 -extensions v3_ca \
        -out "${CACERT}" \
        -nodes || { echo "Failed to sign the CA cert..."; exit 1 ;}
fi

gen_certs () {
    echo "Generating the server private key..."
    # openssl ecparam -genkey -name prime256v1 -noout -out "${KEYFILE}"
    openssl genrsa -out "${KEYFILE}" || { echo "Failed to generate the key..."; exit 1 ;}

    echo "Generating the certficate signing request..."
    openssl req -sha256 -new \
        -batch \
        -config "${ALTNAME_FILE}" -extensions v3_req \
        -key "${KEYFILE}"\
        -nodes \
        -out "${CSRFILE}" || { echo "Failed to gen the cert req..."; exit 1 ;}

    echo "Signing the certificate..."
    openssl ca -config "${ALTNAME_FILE}" \
        -batch \
        -extensions v3_req \
        -days ${CERT_LIFETIME} -notext -md sha256 \
        -in "${CSRFILE}" \
        -out "${CERTFILE}" || { echo "Failed to sign the cert..."; exit 1 ;}

    # Create the chain
    cat "${CERTFILE}" "${CACERT}" > "${CHAINFILE}"

    echo "####################################"
    echo "Successfully created certs!"
    echo "####################################"
}

if [ ! -f "${CERTFILE}" ] || [ ! -f "${CERTFILE}" ] || [ ! -f "${CHAINFILE}" ]; then
    echo "Can't find certs, generating them..."
    gen_certs
else
    # echo "Checking cafile=${CACERT} chainfile=${CHAINFILE}"
    openssl verify -CAfile "${CACERT}" -purpose sslserver "${CHAINFILE}" || gen_certs
fi
echo "Done with cert checks!"
