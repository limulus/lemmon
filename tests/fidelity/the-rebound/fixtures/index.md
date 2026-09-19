---
tags:
  - article
layout: article
author: eric
title: The Rebound
subhead: >-
  I’m rebuilding limulus.net. It’s way past time to have a website again.
date: 2024-11-24 11:35:00 -07:00
hero: upper-falls-letchworth-state-park
---

This used to be a proper website.

It had a blog where I would write on politics. It had a page for a small bit of open source
software I maintained. At one point it even had a photo gallery.

Over the years it moved from Movable Type, to Wordpress, to some custom PHP. And then bit by
bit it became a chore to maintain. “Platforms” like Twitter and Facebook grabbed my
attention and made posting easy.

So limulus.net languished. I deleted it piece by piece. Until all that remained was a single
little homepage with some hand-written HTML. I even [410ed] my blog.

[410ed]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/410

<lem.articlePhoto("middle-falls-letchworth-state-park", placement: "right")>

## The Last Glacial Period

The [Laurentide Ice Sheet] once covered much of North America. Over tens of thousands of
years it accumulated frozen water, spreading further south. It didn’t simply cover land as
its glaciers rolled forward — it scraped, crushed, and ground the earth under enormous
pressure. It and its sibling ice sheets of the period sequestered so much of the planet’s
water that sea levels [fell 120 meters].

[laurentide ice sheet]: https://en.wikipedia.org/wiki/Laurentide_ice_sheet
[fell 120 meters]: https://en.wikipedia.org/w/index.php?title=Post-glacial_rebound&oldid=1257075757#Global_sea_levels

## Elon Musk’s Personal Homepage on the World Wide Web

The reality of the online world that we built didn’t really make itself fully apparent to me
until Elon Musk’s purchase of Twitter. The company was flawed before his ownership but his
gross public treatment of Twitter’s engineers — now his own employees! — prompted me to
swear to myself that I would never work for a company of his nor would I ever purchase a
product from one of his companies given a choice.

Prior to this — like many others — I had scaled back my usage of Facebook after the 2016
U.S. Presidential election. Seeing the under-informed and often outright racist opinions of
distant family members and high school friends put me off using it. At some point Facebook
started down-ranking political posts in their feed algorithm both to avoid a perception of
political bias but also because it was clearly turning off users. Paradoxically this
decreased Facebook’s relevancy to me — what good is it if my posts won’t be seen by people?

I stayed on Twitter for a number of months after the purchase, but when it became apparent
that my values could not abide staying on Twitter I began my exit to decentralized social
media. I’ll be honest, I was surprised such a thing existed or even could exist. My
assumption had always been that social media at a large scale would require centralization.
But I was wrong. It turned out this entire time folks had been working on a [web standard]
to make decentralization work and open-source social media server software like [Mastodon]
was proving it out.

<lem.articlePhoto("mist-middle-falls-letchworth-state-park", placement: "left")>

Getting on Mastodon awakened in me a renewed enthusiasm for the web and having a place of my
own. After a too-brief period on an instance named mastodon.lol, I migrated my account to my
own Mastodon instance at [mastodon.limulus.net]. (Go follow me at [eric@limulus.net][fedi profile]!)

[web standard]: https://activitypub.rocks/
[mastodon]: https://joinmastodon.org/
[mastodon.limulus.net]: https://mastodon.limulus.net/
[fedi profile]: https://limulus.net/@eric

## The Glaciers of New York

As Earth started to warm about 20,000 years ago the Laurentide Ice Sheet began its
recession. But it was not a linear process. Some [14,000 years ago] a glacier flowed south
from current-day Rochester, New York. It covered an area almost as far south as the
Pennsylvania state line. The glaciers were not yet done transforming the land.

[14,000 years ago]: https://experts.illinois.edu/en/publications/evidence-for-a-late-glacial-advance-near-the-beginning-of-the-you

## Billionaires Only Want One Thing and It’s Disgusting

As I write this it is a little over a week after Donald Trump’s second presidential election
win. It seems I am not alone in looking at how things have turned out and being dismayed at
the [outsized influence] billionaires have on us. Trump himself is a billionaire. Musk
became one of Trump’s [largest contributors] and has been warping Twitter further and
further right-wing and [away from free-speech principles]. Peter Thiel, a billionaire early
investor of Meta, believes that [freedom and democracy are incompatible], and is also an
[early contributor] to Vice President-elect J.D. Vance. Rupert Murdoch is a billionaire that
owns a [right-wing entertainment machine] that has done more to create disturbing
information silos than any other entity.

I doubt these people get together to conspire. Surely their egos can only stand each other’s
company for a short time. But they all individually want the same thing: to further cement
their status as oligarchs. The only thing that stands in their way is a well informed
populace with an agreement on basic facts — so they seek to destroy even that.

[outsized influence]: https://newrepublic.com/post/188197/trump-media-information-landscape-fox
[largest contributors]: https://wapo.st/412xKmy
[away from free-speech principles]: https://bsky.app/profile/did:plc:c7ozmxoc5b2ky4iam2o36uic/post/3lb3bewyxj22p
[freedom and democracy are incompatible]: https://www.cato-unbound.org/2009/04/13/peter-thiel/education-libertarian/
[early contributor]: https://www.motherjones.com/politics/2024/07/peter-thiel-donald-trump-vance-hulk-hogan/
[right-wing entertainment machine]: https://en.wikipedia.org/wiki/List_of_assets_owned_by_News_Corp

<lem.articlePhoto("base-of-the-genesee-arch-bridge-letchworth-state-park", placement: "right")>

## A Fair and Famous Stream

Finally the glaciers receded. The land, no longer being crushed by heavy ice sheets,
[rebounded]. New valleys were revealed. The Genesee River — which flows north towards
Rochester and drains into Lake Ontario — found a new route. Over thousands of years this new
route carved Letchworth Gorge, home to a number of impressive waterfalls. This is still new
territory for [our own dear Genesee].

[rebounded]: https://en.wikipedia.org/wiki/Post-glacial_rebound
[our own dear genesee]: https://rbscp.lib.rochester.edu/1816

## Rebuilding limulus.net

What can I do in the face of this threat to my values? Well, there is only so much any one
of us can do to solve the problems of the world. But one small thing I think I can do is
rebuild this space and start regularly creating things to put on it. My hope here is to help
find ways to make “platforms” less necessary, while also promoting ideas like subscribing to
[web feeds] instead of following on LinkedIn, subscribing to a YouTube channel, or
subscribing to [a Substack]. Part of this will — somewhat ironically — involve posting
limulus.net links to some of these platforms.

This is not an approach that I came up with, nor is it even a new idea. It’s known as
[POSSE]: “Publish (on your) Own Site, Syndicate Elsewhere.” Shamefully, I have only recently
come to know about the [IndieWeb] movement that has been championing this approach.

A more technical article about how I am building this site with [Eleventy] and other tools
will inevitably come later. But working on this article and working through some of the
remaining technical challenges has helped alleviate some of my election-outcome despair. I
am back to feeling like there is some hope. A lot of that hope is coming from the potential
the web has to make a rebound after a capitalism-induced ice-age.

[web feeds]: https://aboutfeeds.com/
[a substack]: https://www.anildash.com/2024/11/19/dont-call-it-a-substack/
[posse]: https://indieweb.org/POSSE
[indieweb]: https://indieweb.org
[eleventy]: https://11ty.dev/
