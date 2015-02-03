#ifndef ANNISTYPES_H
#define ANNISTYPES_H

#include <cstdint>
#include <string>
#include <cstring>
#include <limits>

namespace annis
{
  typedef std::uint32_t nodeid_t;

  const std::string annis_ns = "annis4_internal";
  const std::string annis_node_name = "node_name";
  const std::string annis_tok = "tok";

  const unsigned int uintmax = std::numeric_limits<unsigned int>::max();

  struct Edge
  {
    nodeid_t source;
    nodeid_t target;
  };

  enum class ComponentType {COVERAGE, DOMINANCE, POINTING, ORDERING,
                            LEFT_TOKEN, RIGHT_TOKEN,
                            ComponentType_MAX};

  class ComponentTypeHelper
  {
  public:
    static std::string toString(const ComponentType& type)
    {
      switch(type)
      {
      case ComponentType::COVERAGE:
        return "COVERAGE";
        break;
      case ComponentType::DOMINANCE:
        return "DOMINANCE";
        break;
      case ComponentType::POINTING:
        return "POINTING";
        break;
      case ComponentType::ORDERING:
        return "ORDERING";
        break;
      case ComponentType::LEFT_TOKEN:
        return "LEFT_TOKEN";
        break;
      case ComponentType::RIGHT_TOKEN:
        return "RIGHT_TOKEN";
        break;
      default:
        return "UNKNOWN";
      }
    }
/*
    static ComponentType fromString(const std::string& typeAsString)
    {
      for(unsigned int t = (unsigned int)ComponentType::COVERAGE; t < (unsigned int) ComponentType::ComponentType_MAX; t++)
      {
        if(toString((ComponentType) t) == typeAsString)
        {
          return (ComponentType) t;
        }
      }
      return ComponentType::ComponentType_MAX;
    }
*/
  };

  struct Component
  {
    ComponentType type;
    std::string layer;
    std::string name;
  };

  struct AnnotationKey
  {
    std::uint32_t name;
    std::uint32_t ns;
  };

  struct Annotation
  {
    std::uint32_t name;
    std::uint32_t ns;
    std::uint32_t val;
  };

  struct NodeAnnotationKey
  {
    nodeid_t node;
    std::uint32_t anno_name;
    std::uint32_t anno_ns;
  };

  struct TextProperty
  {
    std::uint32_t textID;
    std::uint32_t val;
  };

  struct RelativePosition
  {
    nodeid_t root;
    u_int32_t pos;
  };


  /** combines a node ID and the matched annotation */
  struct Match
  {
//    bool found;
    nodeid_t node;
    Annotation anno;
  };

  /** A combination of two matches together with a flag if a result was found */
  struct BinaryMatch
  {
    bool found;
    Match lhs;
    Match rhs;
  };

  /** Some general statistical numbers specific to a graph component */
  struct GraphStatistic
  {
    /** Average fan out  */
    double avgFanOut;
    /** maximal number of children of a node */
    uint32_t maxFanOut;
    /** maximum length from a root node to a terminal node */
    uint32_t maxDepth;

    bool cyclic;
    bool rootedTree;

    /** Flag to indicate whether the statistics was set */
    bool valid;
  };

  class Init
  {
  public:
    /**
     * @brief initialize an Annotation
     * @param name
     * @param val
     * @param ns
     * @return
     */
    static Annotation initAnnotation(std::uint32_t name = 0, std::uint32_t val=0, std::uint32_t ns=0)
    {
      Annotation result;
      result.name = name;
      result.ns = ns;
      result.val = val;
      return result;
    }

    static Edge initEdge(nodeid_t source, nodeid_t target)
    {
      Edge result;
      result.source = source;
      result.target = target;
      return result;
    }

    static RelativePosition initRelativePosition(nodeid_t node, u_int32_t pos)
    {
      RelativePosition result;
      result.root = node;
      result.pos = pos;
      return result;
    }

    static Match initMatch(const Annotation& anno, nodeid_t node)
    {
      Match result;
      result.node = node;
      result.anno = anno;
      return result;
    }
  };





  inline bool operator==(const Annotation& lhs, const Annotation& rhs)
  {
      return lhs.ns == rhs.ns && lhs.name == rhs.name && lhs.val == rhs.val;
  }

} // end namespace annis

// add implemtations for the types defined here to the std::less operator (and some for the std::hash)
#define ANNIS_STRUCT_COMPARE(a, b) {if(a < b) {return true;} else if(a > b) {return false;}}
namespace std
{

template <>
class hash<annis::Annotation>{
public :
  size_t operator()(const annis::Annotation &a ) const{
    return hash<uint32_t>()(a.ns) ^ hash<uint32_t>()(a.name) ^ hash<uint32_t>()(a.val);
  }
};


template<>
struct less<annis::Component>
{
  bool operator()(const struct annis::Component &a, const struct annis::Component &b) const
  {
    // compare by type
    ANNIS_STRUCT_COMPARE(a.type, b.type);

    // if equal compare by namespace
    ANNIS_STRUCT_COMPARE(a.layer, b.layer);

    // if still equal compare by name
    ANNIS_STRUCT_COMPARE(a.name, b.name);

    // they are equal
    return false;
  }
};

template<>
struct less<annis::AnnotationKey>
{
  bool operator()(const annis::AnnotationKey& a,  const annis::AnnotationKey& b) const
  {
    // compare by name (non lexical but just by the ID)
    ANNIS_STRUCT_COMPARE(a.name, b.name);

    // if equal, compare by namespace (non lexical but just by the ID)
    ANNIS_STRUCT_COMPARE(a.ns, b.ns);

    // they are equal
    return false;
  }
};

template<>
struct less<annis::Annotation>
{
  bool operator()(const annis::Annotation& a,  const annis::Annotation& b) const
  {
    // compare by name (non lexical but just by the ID)
    ANNIS_STRUCT_COMPARE(a.name, b.name);

    // if equal, compare by namespace (non lexical but just by the ID)
    ANNIS_STRUCT_COMPARE(a.ns, b.ns);

    // if still equal compare by value (non lexical but just by the ID)
    ANNIS_STRUCT_COMPARE(a.val, b.val);

    // they are equal
    return false;
  }
};

template<>
struct less<annis::NodeAnnotationKey>
{
  bool operator()(const annis::NodeAnnotationKey& a,  const annis::NodeAnnotationKey& b) const
  {
    // compare by node ID
    ANNIS_STRUCT_COMPARE(a.node, b.node);

    // compare by name (non lexical but just by the ID)
    ANNIS_STRUCT_COMPARE(a.anno_name, b.anno_name);

    // if equal, compare by namespace (non lexical but just by the ID)
    ANNIS_STRUCT_COMPARE(a.anno_ns, b.anno_ns);

    // they are equal
    return false;
  }
};

template<>
struct less<annis::Edge>
{
  bool operator()(const struct annis::Edge &a, const struct annis::Edge &b) const
  {
    // compare by source id
    ANNIS_STRUCT_COMPARE(a.source, b.source);

    // if equal compare by target id
    ANNIS_STRUCT_COMPARE(a.target, b.target);

    // they are equal
    return false;
  }
};

template<>
struct less<annis::TextProperty>
{
  bool operator()(const struct annis::TextProperty &a, const struct annis::TextProperty &b) const
  {
    ANNIS_STRUCT_COMPARE(a.textID, b.textID);
    ANNIS_STRUCT_COMPARE(a.val, b.val);

    // they are equal
    return false;
  }
};

} // end namespace std

#endif // ANNISTYPES_H
