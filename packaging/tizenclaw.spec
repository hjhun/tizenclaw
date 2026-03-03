Name:       tizenclaw
Summary:    TizenClaw Agent System Service App
Version:    1.0.0
Release:    1
Group:      System/Service
License:    Apache-2.0
Source0:    %{name}-%{version}.tar.gz
BuildRequires:  cmake
BuildRequires:  pkgconfig(capi-appfw-service-application)
BuildRequires:  pkgconfig(dlog)
# C++ toolchain is generally available in Tizen

%description
TizenClaw Native Agent running as a System Service Application, utilizing LXC for skills execution.

%prep
%setup -q -n %{name}-%{version}
cp packaging/tizenclaw.manifest .

%build
export CFLAGS="$CFLAGS -Wall"
export CXXFLAGS="$CXXFLAGS -Wall"
export LDFLAGS="$LDFLAGS -Wl,--as-needed"

mkdir -p build
cd build
cmake .. \
    -DCMAKE_INSTALL_PREFIX=/usr
make %{?_smp_mflags}

%install
rm -rf %{buildroot}
cd build
%make_install

# Tizen apps structure
mkdir -p %{buildroot}/opt/usr/apps/org.tizen.tizenclaw/bin
mkdir -p %{buildroot}/opt/usr/apps/org.tizen.tizenclaw/res
mkdir -p %{buildroot}/opt/usr/apps/org.tizen.tizenclaw/data/skills
mkdir -p %{buildroot}/opt/usr/apps/org.tizen.tizenclaw/shared/res

%files
%manifest tizenclaw.manifest
%defattr(-,root,root,-)
/opt/usr/apps/org.tizen.tizenclaw/bin/tizenclaw
/opt/usr/apps/org.tizen.tizenclaw/tizen-manifest.xml
%dir /opt/usr/apps/org.tizen.tizenclaw/data/skills
%dir /opt/usr/apps/org.tizen.tizenclaw/shared/res
%dir /opt/usr/apps/org.tizen.tizenclaw/bin
%dir /opt/usr/apps/org.tizen.tizenclaw/res
%dir /opt/usr/apps/org.tizen.tizenclaw/data
%dir /opt/usr/apps/org.tizen.tizenclaw/shared
%dir /opt/usr/apps/org.tizen.tizenclaw/
