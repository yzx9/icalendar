use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use std::{collections::BTreeMap, fmt, mem};

use crate::properties::*;
use date_time::{format_utc_date_time, naive_date_to_property, parse_utc_date_time};

pub mod alarm;
pub(crate) mod date_time;
mod event;
mod other;
mod todo;
mod venue;

use alarm::*;
use date_time::{CalendarDateTime, DatePerhapsTime};
pub use event::*;
pub use other::*;
pub use todo::*;
pub use venue::*;

const COMP_VEVENT: &str = "VEVENT";
const COMP_VTODO: &str = "VTODO";
const COMP_VVENUE: &str = "VVENUE";
const COMP_VALARM: &str = "VALARM";

const PROP_DTSTAMP: &str = "DTSTAMP";
const PROP_UID: &str = "UID";
const PROP_DTSTART: &str = "DTSTART";
const PROP_DTEND: &str = "DTEND";
const PROP_RECURRENCE_ID: &str = "RECURRENCE-ID";
const PROP_PRIORITY: &str = "PRIORITY";
const PROP_SUMMARY: &str = "SUMMARY";
const PROP_DESCRIPTION: &str = "DESCRIPTION";
const PROP_SEQUENCE: &str = "SEQUENCE";
const PROP_CLASS: &str = "CLASS";
const PROP_URL: &str = "URL";
const PROP_LAST_MODIFIED: &str = "LAST-MODIFIED";
const PROP_CREATED: &str = "CREATED";
const PROP_LOCATION: &str = "LOCATION";

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub(crate) struct InnerComponent {
    pub properties: BTreeMap<String, Property>,
    pub multi_properties: BTreeMap<String, Vec<Property>>,
    pub components: Vec<Other>,
}

impl From<Other> for InnerComponent {
    fn from(val: Other) -> Self {
        val.inner
    }
}

//impl<'a> Into<InnerComponent> for parser::Component<'a> {
//    fn into(self) -> InnerComponent {
//        unimplemented!()
//    }
//}

impl InnerComponent {
    /// End of builder pattern.
    /// copies over everything
    pub fn done(&mut self) -> Self {
        InnerComponent {
            properties: mem::take(&mut self.properties),
            multi_properties: mem::take(&mut self.multi_properties),
            components: mem::take(&mut self.components),
        }
    }

    pub(crate) fn insert_multi(&mut self, property: impl Into<Property>) -> &mut Self {
        let property = property.into();
        let key = property.key().to_owned();

        self.multi_properties
            .entry(key)
            .and_modify(|v| v.push(property.to_owned()))
            .or_insert(vec![property.to_owned()]);
        self
    }

    #[cfg(test)]
    pub fn property_value(&self, key: &str) -> Option<&str> {
        Some(self.properties.get(key)?.value())
    }
}

/// Implemented by everything that goes into a `Calendar`
pub trait Component {
    /// Returns kind of component.
    ///
    ///
    /// Must be ALL CAPS
    /// These are used in the `BEGIN` and `END` line of the component.
    fn component_kind(&self) -> String;

    /// Allows access to the inner properties map.
    fn properties(&self) -> &BTreeMap<String, Property>;

    /// Allows access to the inner's child components.
    fn components(&self) -> &[Other];

    /// Read-only access to `multi_properties`
    fn multi_properties(&self) -> &BTreeMap<String, Vec<Property>>;

    /// Gets the value of a property.
    fn property_value(&self, key: &str) -> Option<&str> {
        Some(self.properties().get(key)?.value())
    }

    /// Writes [`Component`] using [`std::fmt`].
    fn fmt_write<W: fmt::Write>(&self, out: &mut W) -> Result<(), fmt::Error> {
        write_crlf!(out, "BEGIN:{}", self.component_kind())?;

        if !self.properties().contains_key(PROP_DTSTAMP) {
            let now = Utc::now();
            write_crlf!(out, "{}:{}", PROP_DTSTAMP, format_utc_date_time(now))?;
        }

        for property in self.properties().values() {
            property.fmt_write(out)?;
        }

        if !self.properties().contains_key(PROP_UID) {
            write_crlf!(out, "{}:{}", PROP_UID, Uuid::new_v4())?;
        }

        for property in self.multi_properties().values().flatten() {
            property.fmt_write(out)?;
        }

        for component in self.components() {
            component.fmt_write(out)?;
        }

        write_crlf!(out, "END:{}", self.component_kind())?;
        Ok(())
    }

    /// Serializes this component into [`rfc5545`](http://tools.ietf.org/html/rfc5545) again
    ///
    /// # Panic
    /// this can panic if [`std::fmt::write`] returns an Error
    /// use [`Component::try_into_string()`] if you don't like panicking
    fn to_string(&self) -> String {
        self.try_into_string().unwrap()
    }

    /// Serializes this component into [`rfc5545`](http://tools.ietf.org/html/rfc5545) again
    fn try_into_string(&self) -> Result<String, fmt::Error> {
        let mut out_string = String::new();
        self.fmt_write(&mut out_string)?;
        Ok(out_string)
    }

    /// Append a given [`Property`]
    fn append_property(&mut self, property: impl Into<Property>) -> &mut Self;

    /// Append a given [`Component`]
    fn append_component(&mut self, child: impl Into<Other>) -> &mut Self;

    /// Adds a [`Property`] of which there may be many
    fn append_multi_property(&mut self, property: impl Into<Property>) -> &mut Self;

    /// Construct and append a [`Property`]
    fn add_property(&mut self, key: impl Into<String>, val: impl Into<String>) -> &mut Self {
        self.append_property(Property::new(key, val))
    }

    /// Remove a property by its key
    fn remove_property(&mut self, key: &str) -> &mut Self;

    /// Remove a multi-property by its key
    fn remove_multi_property(&mut self, key: &str) -> &mut Self;

    #[deprecated]
    /// Construct and append a [`Property`]
    fn add_property_pre_alloc(&mut self, key: String, val: String) -> &mut Self {
        self.append_property(Property::new(key, val))
    }

    /// Construct and append a [`Property`]
    fn add_multi_property(&mut self, key: &str, val: &str) -> &mut Self {
        self.append_multi_property(Property::new(key, val))
    }

    /// Set the [`DTSTAMP`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.2) [`Property`]
    ///
    /// This must be a UTC date-time value.
    fn timestamp(&mut self, dt: DateTime<Utc>) -> &mut Self {
        self.add_property(PROP_DTSTAMP, format_utc_date_time(dt))
    }

    /// Remove the [`DTSTAMP`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.2) [`Property`]
    fn remove_timestamp(&mut self) -> &mut Self {
        self.remove_property(PROP_DTSTAMP)
    }

    /// Gets the [`DTSTAMP`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.2) property.
    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        parse_utc_date_time(self.property_value(PROP_DTSTAMP)?)
    }

    /// Gets the [`DTSTART`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.4) [`Property`]
    fn get_start(&self) -> Option<DatePerhapsTime> {
        DatePerhapsTime::from_property(self.properties().get(PROP_DTSTART)?)
    }

    /// Gets the [`RECURRENCE-ID`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.4.4) property.
    fn get_recurrence_id(&self) -> Option<DatePerhapsTime> {
        DatePerhapsTime::from_property(self.properties().get(PROP_RECURRENCE_ID)?)
    }

    /// Gets the [`DTEND`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.2) [`Property`]
    fn get_end(&self) -> Option<DatePerhapsTime> {
        DatePerhapsTime::from_property(self.properties().get(PROP_DTEND)?)
    }

    /// Defines the relative priority.
    ///
    /// Ranges from 0 to 10, larger values will be truncated
    fn priority(&mut self, priority: u32) -> &mut Self {
        let priority = std::cmp::min(priority, 10);
        self.add_property(PROP_PRIORITY, priority.to_string())
    }

    /// Removes the relative priority.
    fn remove_priority(&mut self) -> &mut Self {
        self.remove_property(PROP_PRIORITY)
    }

    // /// Add the [`ATTACH`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.1.1) property
    // /// TODO: might have to move to Component
    // pub fn attach(&mut self, attachment: ) -> &mut Self {
    //     todo!()
    //     // self.append_multi_property(todo!())
    // }

    /// Gets the relative priority.
    ///
    /// Ranges from 0 to 10.
    fn get_priority(&self) -> Option<u32> {
        let priority = self.property_value(PROP_PRIORITY)?.parse().ok()?;
        if priority <= 10 {
            Some(priority)
        } else {
            None
        }
    }

    /// Prints to stdout
    fn print(&self) -> Result<(), fmt::Error> {
        let mut out = String::new();
        self.fmt_write(&mut out)?;
        print_crlf!("{}", out);
        Ok(())
    }

    /// Set the summary
    fn summary(&mut self, desc: &str) -> &mut Self {
        self.add_property(PROP_SUMMARY, desc)
    }

    /// Removes the summary
    fn remove_summary(&mut self) -> &mut Self {
        self.remove_property(PROP_SUMMARY)
    }

    /// Gets the summary
    fn get_summary(&self) -> Option<&str> {
        self.property_value(PROP_SUMMARY)
    }

    /// Set the description
    fn description(&mut self, desc: &str) -> &mut Self {
        self.add_property(PROP_DESCRIPTION, desc)
    }

    /// Removes the description
    fn remove_description(&mut self) -> &mut Self {
        self.remove_property(PROP_DESCRIPTION)
    }

    /// Gets the description
    fn get_description(&self) -> Option<&str> {
        self.property_value(PROP_DESCRIPTION)
    }

    ///// Set the description
    ///// TODO `Attendee` needs to be its own type
    //fn attendee(&mut self, desc: &str) -> &mut Self {
    //    self.add_multi_property("ATTENDEE", desc) // multi_properties should be a multi-map
    //}

    /// Set the UID
    fn uid(&mut self, uid: &str) -> &mut Self {
        self.add_property(PROP_UID, uid)
    }

    /// Gets the UID
    fn get_uid(&self) -> Option<&str> {
        self.property_value(PROP_UID)
    }

    /// Set the sequence
    fn sequence(&mut self, sequence: u32) -> &mut Self {
        self.add_property(PROP_SEQUENCE, sequence.to_string())
    }

    /// Removes the sequence
    fn remove_sequence(&mut self) -> &mut Self {
        self.remove_property(PROP_SEQUENCE)
    }

    /// Gets the sequence
    fn get_sequence(&self) -> Option<u32> {
        self.property_value(PROP_SEQUENCE)
            .and_then(|s| s.parse().ok())
    }

    /// Set the visibility class
    fn class(&mut self, class: Class) -> &mut Self {
        self.append_property(class)
    }

    /// Removes the visibility class
    fn remove_class(&mut self) -> &mut Self {
        self.remove_property(PROP_CLASS)
    }

    /// Gets the visibility class
    fn get_class(&self) -> Option<Class> {
        Class::from_str(self.property_value(PROP_CLASS)?)
    }

    /// Sets the URL.
    fn url(&mut self, url: &str) -> &mut Self {
        self.add_property(PROP_URL, url)
    }

    /// Removes the URL.
    fn remove_url(&mut self) -> &mut Self {
        self.remove_property(PROP_URL)
    }

    /// Gets the URL.
    fn get_url(&self) -> Option<&str> {
        self.property_value(PROP_URL)
    }

    /// Set the [`LAST-MODIFIED`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.3) [`Property`]
    ///
    /// This must be a UTC date-time value.
    fn last_modified(&mut self, dt: DateTime<Utc>) -> &mut Self {
        self.add_property(PROP_LAST_MODIFIED, format_utc_date_time(dt))
    }

    /// Removes the [`LAST-MODIFIED`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.3) [`Property`]
    fn remove_last_modified(&mut self) -> &mut Self {
        self.remove_property(PROP_LAST_MODIFIED)
    }

    /// Gets the [`LAST-MODIFIED`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.3) property.
    fn get_last_modified(&self) -> Option<DateTime<Utc>> {
        parse_utc_date_time(self.property_value(PROP_LAST_MODIFIED)?)
    }

    /// Set the [`CREATED`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.1) [`Property`]
    ///
    /// This must be a UTC date-time value.
    fn created(&mut self, dt: DateTime<Utc>) -> &mut Self {
        self.add_property(PROP_CREATED, format_utc_date_time(dt))
    }

    /// Removes the [`CREATED`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.1) [`Property`]
    fn remove_created(&mut self) -> &mut Self {
        self.remove_property(PROP_CREATED)
    }

    /// Gets the [`CREATED`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.7.1) property.
    fn get_created(&self) -> Option<DateTime<Utc>> {
        parse_utc_date_time(self.property_value(PROP_CREATED)?)
    }
}

/// Common trait of [`Event`] and [`Todo`]
pub trait EventLike: Component {
    /// Set the [`DTSTART`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.4) [`Property`]
    ///
    /// See [`DatePerhapsTime`] for info how are different [`chrono`] types converted automatically.
    fn starts<T: Into<DatePerhapsTime>>(&mut self, dt: T) -> &mut Self {
        let calendar_dt = dt.into();
        self.append_property(calendar_dt.to_property(PROP_DTSTART))
    }

    /// removes the [`DTSTART`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.4) [`Property`]
    fn remove_starts(&mut self) -> &mut Self {
        self.remove_property(PROP_DTSTART)
    }

    /// Set the [`DTEND`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.2) [`Property`]
    ///
    /// See [`DatePerhapsTime`] for info how are different [`chrono`] types converted automatically.
    fn ends<T: Into<DatePerhapsTime>>(&mut self, dt: T) -> &mut Self {
        let calendar_dt = dt.into();
        self.append_property(calendar_dt.to_property(PROP_DTEND))
    }

    /// Removes the [`DTEND`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.2) [`Property`]
    fn remove_ends(&mut self) -> &mut Self {
        self.remove_property(PROP_DTEND)
    }

    /// Sets the [`RECURRENCE-ID`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.4.4)
    /// property.
    ///
    /// See [`DatePerhapsTime`] for info how are different [`chrono`] types converted automatically.
    fn recurrence_id<T: Into<DatePerhapsTime>>(&mut self, dt: T) -> &mut Self {
        let calendar_dt = dt.into();
        self.append_property(calendar_dt.to_property(PROP_RECURRENCE_ID))
    }

    /// Removes the [`RECURRENCE-ID`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.4.4)
    fn remove_recurrence_id(&mut self) -> &mut Self {
        self.remove_property(PROP_RECURRENCE_ID)
    }

    /// Set the [`DTSTART`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.4) [`Property`]
    /// and [`DTEND`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.8.2.2) [`Property`],
    /// date only
    fn all_day(&mut self, date: NaiveDate) -> &mut Self {
        self.append_property(naive_date_to_property(date, PROP_DTSTART))
            .append_property(naive_date_to_property(date, PROP_DTEND))
    }

    /// Set the LOCATION with a VVENUE UID
    /// iCalender venue draft
    fn venue(&mut self, location: &str, venue_uid: &str) -> &mut Self {
        self.append_property(
            Property::new(PROP_LOCATION, location)
                .append_parameter(Parameter::new(COMP_VVENUE, venue_uid))
                .done(),
        );
        self
    }

    /// Set the LOCATION
    /// 3.8.1.7.  Location
    fn location(&mut self, location: &str) -> &mut Self {
        self.add_property(PROP_LOCATION, location)
    }

    /// Removes the LOCATION with a VVENUE UID
    fn remove_location(&mut self) -> &mut Self {
        self.remove_property(PROP_LOCATION)
    }

    /// Gets the location
    fn get_location(&self) -> Option<&str> {
        self.property_value(PROP_LOCATION)
    }

    /// Set the ALARM for this event
    /// [3.6.6.  Alarm Component](https://datatracker.ietf.org/doc/html/rfc5545#section-3.6.6)
    fn alarm<A: Into<Alarm>>(&mut self, alarm: A) -> &mut Self {
        let alarm: Alarm = alarm.into();
        self.append_component(alarm)
    }
}

macro_rules! event_impl {
    ($t:ty) => {
        impl EventLike for $t {}
    };
}

macro_rules! component_impl {
    ($t:ty, $kind:expr) => {
        impl Component for $t {
            /// Tells you what kind of [`Component`] this is
            ///
            /// Might be [`VEVENT`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.6.1), [`VTODO`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.6.2), [`VALARM`](https://datatracker.ietf.org/doc/html/rfc5545#section-3.6.6) etc
            fn component_kind(&self) -> String {
                $kind
            }

            /// Read-only access to `properties`
            fn properties(&self) -> &BTreeMap<String, Property> {
                &self.inner.properties
            }

            /// Read-only access to `properties`
            fn components(&self) -> &[Other] {
                &self.inner.components
            }

            /// Read-only access to `multi_properties`
            fn multi_properties(&self) -> &BTreeMap<String, Vec<Property>> {
                &self.inner.multi_properties
            }

            /// Adds a [`Property`]
            fn append_property(&mut self, property: impl Into<Property>) -> &mut Self {
                let property = property.into();
                self.inner
                    .properties
                    .insert(property.key().to_owned(), property);
                self
            }

            /// Appends a [`Component`]
            fn append_component(&mut self, child: impl Into<Other>) -> &mut Self {
                self.inner.components.push(child.into());
                self
            }

            /// Adds a [`Property`] of which there may be many
            fn append_multi_property(&mut self, property: impl Into<Property>) -> &mut Self {
                self.inner.insert_multi(property);
                self
            }

            /// Removes a [`Property`] by its key if it exists
            fn remove_property(&mut self, key: &str) -> &mut Self {
                self.inner.properties.remove(key);
                self
            }

            /// Removes a multi-property by its key if it exists
            fn remove_multi_property(&mut self, key: &str) -> &mut Self {
                self.inner.multi_properties.remove(key);
                self
            }
        }

        impl From<InnerComponent> for $t {
            fn from(inner: InnerComponent) -> $t {
                Self { inner }
            }
        }
        impl From<$t> for Other {
            fn from(val: $t) -> Self {
                (val.component_kind(), val.inner).into()
            }
        }

        impl TryInto<String> for $t {
            type Error = std::fmt::Error;

            fn try_into(self) -> Result<String, Self::Error> {
                self.try_into_string()
            }
        }
    };
}

component_impl! { Event, String::from(COMP_VEVENT) }
event_impl! { Event }

component_impl! { Todo , String::from(COMP_VTODO)}
event_impl! { Todo}

component_impl! { Venue , String::from(COMP_VVENUE)}
component_impl! { Alarm, String::from(COMP_VALARM) }

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn get_url() {
        let url = "http://hoodie.de/";
        let event = Event::new().url(url).done();

        let serialized = event.to_string();
        let reparsed =
            Other::from(crate::parser::Component::<'_>::try_from(serialized.as_str()).unwrap());

        assert_eq!(event.get_url(), Some(url));
        assert_eq!(reparsed.get_url(), Some(url));
    }

    #[test]
    fn get_properties_unset() {
        let event = Event::new();
        assert_eq!(event.get_priority(), None);
        assert_eq!(event.get_summary(), None);
        assert_eq!(event.get_description(), None);
        assert_eq!(event.get_location(), None);
        assert_eq!(event.get_uid(), None);
        assert_eq!(event.get_class(), None);
        assert_eq!(event.get_timestamp(), None);
        assert_eq!(event.get_last_modified(), None);
        assert_eq!(event.get_created(), None);
        assert_eq!(event.get_url(), None);
    }

    #[test]
    fn get_properties_set() {
        let event = Event::new()
            .priority(5)
            .summary("summary")
            .description("description")
            .location("location")
            .uid("uid")
            .class(Class::Private)
            .url("http://some.test/url")
            .done();
        assert_eq!(event.get_priority(), Some(5));
        assert_eq!(event.get_summary(), Some("summary"));
        assert_eq!(event.get_description(), Some("description"));
        assert_eq!(event.get_location(), Some("location"));
        assert_eq!(event.get_uid(), Some("uid"));
        assert_eq!(event.get_class(), Some(Class::Private));
        assert_eq!(event.get_url(), Some("http://some.test/url"));
    }

    #[test]
    fn get_properties_remove() {
        let mut event = Event::new()
            .priority(5)
            .summary("summary")
            .description("description")
            .location("location")
            .uid("uid")
            .class(Class::Private)
            .url("http://some.test/url")
            .done();
        assert_eq!(event.get_priority(), Some(5));
        assert_eq!(event.get_summary(), Some("summary"));
        assert_eq!(event.get_description(), Some("description"));
        assert_eq!(event.get_location(), Some("location"));
        assert_eq!(event.get_uid(), Some("uid"));
        assert_eq!(event.get_class(), Some(Class::Private));
        assert_eq!(event.get_url(), Some("http://some.test/url"));

        event
            .remove_priority()
            .remove_summary()
            .remove_description()
            .remove_location()
            .remove_class()
            .remove_url();
        assert_eq!(event.get_priority(), None);
        assert_eq!(event.get_summary(), None);
        assert_eq!(event.get_description(), None);
        assert_eq!(event.get_location(), None);
        assert_eq!(event.get_class(), None);
        assert_eq!(event.get_url(), None);
    }

    #[test]
    fn get_date_times_naive() {
        let naive_date_time = NaiveDate::from_ymd_opt(2001, 3, 13)
            .unwrap()
            .and_hms_opt(14, 15, 16)
            .unwrap();
        let event = Event::new()
            .starts(naive_date_time)
            .ends(naive_date_time)
            .done();
        assert_eq!(event.get_start(), Some(naive_date_time.into()));
        assert_eq!(event.get_end(), Some(naive_date_time.into()));
    }

    #[test]
    fn get_date_times_utc() {
        let utc_date_time = Utc.with_ymd_and_hms(2001, 3, 13, 14, 15, 16).unwrap();
        let event = Event::new()
            .timestamp(utc_date_time)
            .last_modified(utc_date_time)
            .created(utc_date_time)
            .starts(utc_date_time)
            .ends(utc_date_time)
            .done();
        assert_eq!(event.get_timestamp(), Some(utc_date_time));
        assert_eq!(event.get_last_modified(), Some(utc_date_time));
        assert_eq!(event.get_created(), Some(utc_date_time));
        assert_eq!(event.get_start(), Some(utc_date_time.into()));
        assert_eq!(event.get_end(), Some(utc_date_time.into()));
    }

    #[test]
    fn get_date_times_tzid() {
        let date_time = NaiveDate::from_ymd_opt(2001, 3, 13)
            .unwrap()
            .and_hms_opt(14, 15, 16)
            .unwrap();
        let date_time_tzid = CalendarDateTime::WithTimezone {
            date_time,
            tzid: "Pacific/Auckland".to_string(),
        };
        let event = Event::new()
            .starts(date_time_tzid.clone())
            .ends(date_time_tzid.clone())
            .done();
        assert_eq!(event.get_start(), Some(date_time_tzid.clone().into()));
        assert_eq!(event.get_end(), Some(date_time_tzid.into()));
    }

    #[test]
    fn get_dates_naive() {
        let naive_date = NaiveDate::from_ymd_opt(2001, 3, 13).unwrap();
        let event = Event::new().starts(naive_date).ends(naive_date).done();
        assert_eq!(event.get_start(), Some(naive_date.into()));
        assert_eq!(event.get_end(), Some(naive_date.into()));
    }
}
